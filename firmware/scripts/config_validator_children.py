"""Private complete normal-child relations for the selected SizeZ visitor.

Actual callers still bind typed receivers/scalars, initialized prefixes and
physical live storage. These functions cannot validate Config or issue an
owner. Bands' pending diagnostic cut remains a separate unresolved alternative.
"""
import re
from dataclasses import replace
from config_finite import _literal_body
from config_helpers import _helper_function
from config_refill_children import _readonly
from config_regions import Storage,View,Argument,require_disjoint
from config_roots import root_location
from config_stack import canonical_symbol,_typed_function_extent
from config_validator_child_contracts import NORMAL_CHILDREN,NORMAL_ROLES
from validation import require


def _object(objects,address,descriptor):
    if descriptor['kind']=='source_location':
        require(descriptor['role']=='bands_slice','normal validator child borrows another diagnostic Location')
        root_location(objects,address,'bands_slice');return ()
    require(descriptor['kind']=='readonly_values' and descriptor['role'] in ('linear5','linear6','printable_ascii_range'),
            'unreviewed normal-child readonly object')
    raw=bytes.fromhex(descriptor['hex'])
    require(_readonly(objects,address,len(raw),descriptor['alignment'])==raw,
            'normal-child source table/range differs')
    return ()


def _normal_body(objects,name,instructions,family,_ancestry=()):
    require(family in NORMAL_CHILDREN and name in objects.elf.functions,'unknown normal validator child')
    start=objects.elf.functions[name][0]&~1;contract=NORMAL_CHILDREN[family]
    require(start not in _ancestry,'normal child gains an activation cycle')
    if family=='text_restore':
        require(re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name) and len(_ancestry)==1,
                'text restore has no actual nonempty parent frame')
        parent_names=[n for n,e in objects.elf.functions.items() if e[0]&~1==_ancestry[0] and
                      canonical_symbol(n)==next(k for k,v in NORMAL_ROLES.items() if v=='text_valid')]
        require(len(parent_names)==1,'text restoring tail belongs to another parent')
        role='@outline:text_restore'
    else:
        role=canonical_symbol(name)
        require(NORMAL_ROLES.get(role)==family,'normal child receiver/trait/raw role differs')
    rows={a:replace(r,operands=r.operands.split('@',1)[0].split('<',1)[0].strip())
          for a,r in instructions.items()};nested={}
    def function(objects,address,raw_role):
        if address==start and family=='text_restore':
            extent,raw=_typed_function_extent(objects,address,raw_role)
            require((extent,objects.sizes.get(address,0))==(6,0),'text restoring tail changes extent/frame')
            return extent,0,raw
        return _helper_function(objects,address,raw_role)
    def outline(objects,child,records,raw_role,ancestry):
        require(family=='text_valid' and raw_role=='@outline:text_restore' and ancestry==(start,),
                'normal validator child gains another parent-owned outline')
        proof=_normal_body(objects,child,records,'text_restore',ancestry);nested['text_restore']=proof;return proof
    proof=_literal_body(objects,name,rows,role,family,contract,_object,outline,_ancestry,
                        function_consumer=function)
    require(not proof['indirect_sites'],'normal validator child gains an indirect target')
    for offset,operation,target,raw_role,frame in proof['ordinary_edges']:
        if raw_role.startswith('@outline:'):continue
        if family=='bands':
            require((offset,operation,raw_role,frame)==
                    (20,'bl','_RNvNtNtCs_4core5slice5index16slice_index_fail',32),
                    'bands changes its exact pending diagnostic-cut alternative')
            continue
        wanted='range_u8' if family=='text_valid' else 'luminance' if family=='contrasts' else None
        require(wanted is not None and NORMAL_ROLES.get(raw_role)==wanted and
                ((family=='text_valid' and offset==34) or (family=='contrasts' and offset in (6,14))),
                'normal child gains another callee/argument phase')
        extent,actual,child=_helper_function(objects,target,raw_role)
        nested[offset]=_normal_body(objects,child,instructions,wanted,_ancestry+(start,))
        require((extent,actual)==(nested[offset]['extent'],nested[offset]['frame']),
                'normal nested child changes complete metadata')
    expected={'text_valid':{34,'text_restore'},'contrasts':{6,14}}.get(family,set())
    require(set(nested)==expected,'normal child omits an exact dependency/return')
    physical={'range_u16':((8,),),'range_u8':((8,),),'text_valid':((0,),(32,),(32,8)),
        'text_restore':((0,),),'contrasts':((16,),(16,16)),'luminance':((16,),),
        'channel_domain':((8,),),'bands':((0,),)}[family]
    proof.update(children=nested,normal_physical_paths=physical,
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp','d8-d15'),
        external_writes=(),architectural_LR_preserved=False,
        original_return_PC_restored=True,
        parent_frame_required=(32,'high12 then low20 exact text saves') if family=='text_restore' else None,
        diagnostic_pending=family=='bands')
    return proof


def _normal_storage(proof,context,readonly):
    """Map every nested save write below actual Q and protect live inputs."""
    q=context.frame;readonly=tuple(readonly)
    require(all(isinstance(v,View) and not v.writable and v.storage.phase==q.phase for v in readonly),
            'normal child reads a stale/mutable source epoch')
    intervals=[]
    for path in proof['normal_physical_paths']:
        depth=0
        for ordinal,size in enumerate(path):
            if not size:continue
            active=Storage(q.space,q.identity+':'+proof['shape']+':'+str(ordinal),
                           q.start-depth-size,size,4,q.phase,True);depth+=size
            for retained in (q,context.caller,context.config.storage)+context.protected:
                require_disjoint((active,retained))
            for view in readonly:
                live=Storage(view.storage.space,'normal child readonly '+str(view.start),view.start,view.extent,1,q.phase,False)
                require_disjoint((active,live))
            intervals.append((active.start,active.start+active.extent))
    return tuple(sorted(set(intervals)))
