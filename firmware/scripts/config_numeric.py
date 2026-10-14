"""Finite numeric formatting witnesses; caller value/options/sink closure required."""
import re
from config_numeric_contracts import NUMERIC_VARIANTS
from config_stack import canonical_symbol,typed_function
from llvm_stack import mapping_ranges
from validation import require

NUMERIC_OBJECTS = {
    'decimal_pairs': b''.join(f'{i:02}'.encode() for i in range(100)),
    'lower_hex_digits': b'0123456789abcdef', 'upper_hex_digits': b'0123456789ABCDEF',
    'hex_prefix': b'0x', 'bool_true': b'true', 'bool_false': b'false',
}

# Offsets are invocation sites, not a name-only permission to execute a callee.
# Each route requires the actual incoming default options to be bound upstream.
DEFAULT_TRANSFERS = {
    ('<usize as core::fmt::Debug>::fmt',278): (0x1e,),
    ('<usize as core::fmt::Debug>::fmt',38): (0x12,),
    ('<&u8 as core::fmt::Debug>::fmt',212): (0xcc,),
    ('<&u8 as core::fmt::Debug>::fmt',40): (0x14,),
    ('<&u16 as core::fmt::Debug>::fmt',234): (0x18,),
    ('<&u16 as core::fmt::Debug>::fmt',40): (0x14,),
    ('<&bool as core::fmt::Debug>::fmt',50): (0x1c,0x2e),
    ('<&bool as core::fmt::Debug>::fmt',14): (0x0a,),
    ('<bool as core::fmt::Display>::fmt',50): (0x1a,0x2e),
    ('<u8 as core::fmt::Display>::fmt',96): (0x58,),
    ('<u16 as core::fmt::Display>::fmt',158): (0x96,),
    ('<u32 as core::fmt::Display>::fmt',230): (0xda,),
    ('<core::fmt::Formatter>::pad',452): (0x126,),
    ('<core::fmt::Formatter>::pad',320): (0xd2,),
    ('<core::fmt::Formatter>::pad_integral',412): (0x94,0xae),
    ('<core::fmt::Formatter>::pad_integral',396): (0x8a,0xa4),
    ('<core::fmt::Formatter>::pad_integral::write_prefix',68): (),
}


def numeric_format(objects,name,instructions):
    """Check complete emitted bodies, including excluded custom option paths.

    No numeric path is admitted merely by this result: its caller must prove
    originating scalar/reference extent, same live sink and actual defaults.
    Retain current parent frames across tails without claiming residency.
    """
    role=canonical_symbol(name)
    require(role in NUMERIC_VARIANTS and name in objects.elf.functions,'unreviewed numeric interface')
    start=objects.elf.functions[name][0]&~1
    extent,frame,_=typed_function(objects,start,role)
    variants=[v for v in NUMERIC_VARIANTS[role] if v[1:3]==(extent,frame)]
    require(len(variants)==1,'unreviewed numeric emitted shape')
    kind,_,_,mapping,grammar=variants[0]
    require(mapping_ranges(objects.elf,name)==[(start+a,start+b,k) for a,b,k in mapping],
            'numeric body mapping differs')
    raw=objects.read_only(start,extent);rows=sorted(instructions,key=lambda r:r.address)
    code={a for lo,hi,k in mapping if k=='t' for a in range(lo,hi)}
    data={a for lo,hi,k in mapping if k=='d' for a in range(lo,hi)}
    instructions_grammar=[];data_covered=set()
    for line in grammar:
        offset=int(line.split(' ',1)[0],16)
        if ' DATA ' in line:
            value=bytes.fromhex(line.split(' DATA ',1)[1]);points=set(range(offset,offset+len(value)))
            require(points<=data and not points&data_covered and raw[offset:offset+len(value)]==value,
                    'numeric mapped table differs')
            data_covered|=points
        else:instructions_grammar.append(line)
    require(data_covered==data and len(rows)==len(instructions_grammar),'numeric instruction/data inventory differs')
    covered=set();pairs={};transfers=[]
    for record,line in zip(rows,instructions_grammar):
        offset=int(line.split(' ',1)[0],16);mnemonic,expected=line.split(' ',1)[1].split('\t',1)
        points=set(range(offset,offset+len(record.raw)))
        require(record.address==start+offset and len(record.raw) in (2,4) and points<=code and
                not points&covered and record.raw==raw[offset:offset+len(record.raw)],'numeric current bytes/coverage differ')
        covered|=points
        require(record.mnemonic==mnemonic,'numeric instruction differs')
        actual=record.operands.split('@',1)[0].strip()
        producer=re.search(r'@(decimal_pairs|lower_hex_digits|upper_hex_digits|hex_prefix|bool_true|bool_false)\.(low|high)',expected)
        local=re.search(r'@self\+([0-9a-f]+)',expected)
        external=re.search(r'@role:(\S+)',expected)
        if producer:
            match=re.fullmatch(r'(r[0-9]+|lr), #(0x[0-9a-f]+|[0-9]+)',actual)
            require(match is not None and int(match[2],0)<=0xffff,'invalid numeric readonly producer')
            key=producer[1];part=producer[2]
            require(part not in pairs.setdefault(key,{}),'ambiguous numeric readonly producer')
            pairs[key][part]=int(match[2],0);actual=match[1]+', '+producer[0]
        elif local:
            target=re.search(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(target is not None and int(target[1],16)==start+int(local[1],16),'numeric local target differs')
            actual=actual[:target.start()]+local[0]
        elif external:
            target=re.match(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(target is not None,'unresolved numeric ordinary transfer')
            address=int(target[1],16);_,child_frame,_=typed_function(objects,address,external[1])
            transfers.append((offset,mnemonic,address,external[1],child_frame));actual=external[0]
        elif mnemonic in ('bx','blx') and actual!='lr':
            transfers.append((offset,mnemonic,None,actual,None))
        require(actual==expected,'numeric operand/range/buffer/interface differs')
    require(covered==code,'incomplete numeric code coverage')
    resolved={}
    for key,pair in pairs.items():
        require(set(pair)=={'low','high'},'incomplete numeric readonly producer')
        address=pair['low']|(pair['high']<<16);expected=NUMERIC_OBJECTS[key]
        require(objects.read_only(address,len(expected))==expected,'numeric table/string differs')
        resolved[key]=address
    active=DEFAULT_TRANSFERS[(kind,extent)]
    require(set(active)<=set(t[0] for t in transfers),'missing numeric default interface')
    return dict(role=role,kind=kind,function=start,extent=extent,frame=frame,objects=resolved,
                transfers=tuple(transfers),default_sites=active,
                excluded_sites=tuple(t[0] for t in transfers if t[0] not in active),
                requires_options=(0x60000020,0,0),requires_originating_value_and_live_sink=True)
