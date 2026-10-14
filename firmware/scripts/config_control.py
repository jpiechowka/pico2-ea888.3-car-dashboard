"""Finite ControlError case/bridge bodies; originating context is separate."""
import re
import struct
from config_control_contracts import CONTROL_GRAMMARS
from config_stack import canonical_symbol,typed_function,enum_dispatch
from llvm_stack import mapping_ranges
from validation import require

CONTROL_CASES=('HoldDuration','ReservedRecovery','IncompatibleButton','IncompatibleEncoderClick',
               'IncompatibleEncoderRotation','UnreachableScreen','MissingViewNavigation')
CONTROL_TAGS=((2,),(3,),(0,1),(5,),(6,),(7,),(8,))


def control_object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind=='bytes':
        expected=bytes.fromhex(descriptor['hex'])
        require(objects.read_only(address,len(expected))==expected,'ControlError label/length/delimiter differs')
        return ()
    if kind=='pointer_table':
        labels=[v.encode() for v in descriptor['labels']]
        require(address%4==0,'unaligned control label pointer table')
        pointers=struct.unpack('<'+'I'*len(labels),objects.read_only(address,4*len(labels)))
        for pointer,label in zip(pointers,labels):
            require(objects.read_only(pointer,len(label))==label,'control indexed label differs')
        return ()
    if kind=='callback':
        require(address&1,'control raw callback is not Thumb')
        extent,frame,_=typed_function(objects,address&~1,descriptor['role'])
        require(frame==descriptor['frame'],'control raw callback frame differs')
        return ((address&~1,descriptor['role'],extent,frame),)
    require(kind=='vtable' and address%4==0,'unreviewed control object interface')
    roles=descriptor['roles'];words=struct.unpack('<'+'I'*(3+len(roles)),objects.read_only(address,4*(3+len(roles))))
    require(words[:3]==tuple(descriptor['layout']),'control typed table layout differs')
    result=[]
    for pointer,role in zip(words[3:],roles):
        require(pointer&1,'control typed table callback is not Thumb')
        extent,frame,_=typed_function(objects,pointer&~1,role)
        result.append((pointer&~1,role,extent,frame))
    return tuple(result)


def control_body(objects,name,instructions,outline_role=None):
    """Match every current byte/operand and typed object, including pretty paths.

    Complete local predecessor and return-cell shapes are fixed by the finite
    grammar. This does not establish the caller's live value/sink/defaults or
    exempt any returned callback/ordinary child from whole closure/accounting.
    Outline effects only have meaning under their checked parent call sites.
    """
    require(name in objects.elf.functions,'missing control body')
    start=objects.elf.functions[name][0]&~1;raw_role=canonical_symbol(name)
    extent,frame,_=typed_function(objects,start,raw_role)
    if outline_role:
        require(outline_role in ('@restore_control','@control_sink2','@control_pad2') and
                re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'unreviewed control outline role')
    role=outline_role or raw_role;key=role+':'+str(extent)+':'+str(frame)
    require(key in CONTROL_GRAMMARS,'unreviewed complete control body shape')
    contract=CONTROL_GRAMMARS[key];mapping=[(start+a,start+b,k) for a,b,k in contract['mapping']]
    require(mapping_ranges(objects.elf,name)==mapping,'control code/data mapping differs')
    rows=sorted((r for a,r in instructions.items() if start<=a<start+extent),key=lambda r:r.address)
    raw=objects.read_only(start,extent);code={a for lo,hi,k in mapping if k=='t' for a in range(lo,hi)}
    require(len(rows)==len(contract['grammar'])==len(contract['widths']),'control instruction inventory differs')
    covered=set();producers={};edges=[];indirect=[]
    for row,line,width in zip(rows,contract['grammar'],contract['widths']):
        at,operation=line.split(' ',1);mnemonic,expected=operation.split('\t',1);offset=int(at,16)
        points=set(range(row.address,row.address+len(row.raw)))
        require(row.address==start+offset and len(row.raw)==width and points<=code and not points&covered and
                raw[offset:offset+width]==row.raw,'control current bytes/coverage differ')
        covered|=points;actual=row.operands.split('@',1)[0].strip()
        require(row.mnemonic==mnemonic,'control instruction/return/selector differs')
        producer=re.search(r'@object([0-9]+)\.(low|high)',expected)
        local=re.search(r'@self\+([0-9a-f]+)',expected)
        target_role=expected.removeprefix('@role:') if expected.startswith('@role:') else None
        if producer:
            match=re.fullmatch(r'(r[0-9]+|lr), #(0x[0-9a-f]+|[0-9]+)',actual)
            require(match is not None and int(match[2],0)<=65535,'invalid control typed object producer')
            ordinal,part=int(producer[1]),producer[2]
            require(part not in producers.setdefault(ordinal,{}),'ambiguous control object producer')
            producers[ordinal][part]=int(match[2],0);actual=match[1]+', '+producer[0]
        elif local:
            match=re.search(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(match is not None and int(match[1],16)==start+int(local[1],16),'control local predecessor differs')
            actual=actual[:match.start()]+local[0]
        elif target_role:
            match=re.match(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(match is not None,'missing control ordinary target')
            target=int(match[1],16)
            if target_role.startswith('@'):
                names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
                require(names,'missing control outline target')
                child=control_body(objects,names[0],instructions,target_role);child_frame=child['frame']
            else:_,child_frame,_=typed_function(objects,target,target_role)
            edges.append((offset,mnemonic,target,target_role,child_frame));actual=expected
        elif mnemonic in ('bx','blx') and actual!='lr':indirect.append((offset,mnemonic,actual))
        require(actual==expected,'control value/cell/callback/default/epilogue operands differ')
    require(covered==code,'incomplete control mapped code')
    data={a for lo,hi,k in mapping if k=='d' for a in range(lo,hi)};pool_bytes=set();values=[];callbacks=[]
    current=bytearray(raw);by_offset={r.address-start:r for r in rows}
    for ordinal,descriptor in enumerate(contract['objects']):
        if 'pool' in descriptor:
            pool=start+descriptor['pool'];points=set(range(pool,pool+4));row=by_offset[descriptor['site']]
            match=re.fullmatch(re.escape(descriptor['register'])+r', \[pc, #(0x[0-9a-f]+|[0-9]+)\]',row.operands.split('@',1)[0].strip())
            require(match is not None and ((row.address+4)&~3)+int(match[1],0)==pool and points<=data,
                    'control PC object pool ownership differs')
            value=struct.unpack('<I',objects.read_only(pool,4))[0]
            pool_bytes|=points;current[descriptor['pool']:descriptor['pool']+4]=bytes(4)
        else:
            require(ordinal in producers and set(producers[ordinal])=={'low','high'},'incomplete control object pair')
            pair=producers[ordinal];value=pair['low']|(pair['high']<<16)
        values.append(value);callbacks.extend(control_object(objects,value,descriptor))
    require(set(producers)=={i for i,v in enumerate(contract['objects']) if 'pool' not in v},'unbound control object pair')
    fixed=set()
    for offset,value in contract['data']:
        value=bytes.fromhex(value);points=set(range(start+offset,start+offset+len(value)))
        require(points<=data and not points&fixed and current[offset:offset+len(value)]==value,'control TBB/pool data differs')
        fixed|=points
    require(fixed==data,'unbound control mapped data')
    cases=()
    if '12ControlError' in role:
        dispatch=enum_dispatch(objects,name,rows)
        cases=tuple(zip(CONTROL_CASES,CONTROL_TAGS,dispatch['entries']))
    return dict(role=role,function=start,extent=extent,frame=frame,objects=tuple(values),callbacks=tuple(callbacks),
                ordinary_edges=tuple(edges),indirect_sites=tuple(indirect),cases=cases,
                source_tags=(0,1,2,3,5,6,7,8) if cases else (),requires_value_source_and_live_sink=True,
                requires_options=(0x60000020,0,0),requires_pretty_inventory_exclusion=True,
                requires_parent_outline_abi=outline_role)
