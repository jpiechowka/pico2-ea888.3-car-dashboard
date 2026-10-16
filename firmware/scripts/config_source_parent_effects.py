"""Authored parent write envelopes for the complete decoded Sources family.

These current byte/geometry checks grant no active phase or typed value.
The actual producer must also join source, chronological pointer lifetimes,
child preservation, guarded partial writes and reached access requirements.
"""
from config_regions import Storage,borrow,require_writes,require_disjoint
from config_decode_effects import _object
from config_abstract import SequenceInterface
from config_source_contracts import GRAMMARS
from validation import require


# (site, owner coordinate, offset, physical bytes, authored category).
# Transcribed from retained current literals and source/pointer evidence;
# not learned at runtime from a newly supplied image.
PARENT_WRITES=(
    (4388, 'V', 284, 4, 'source_tag_capture'),
    (4422, 'V', 1460, 16, '_CARRIER_COPY'),
    (4428, 'V', 1478, 1, '_CARRIER_COPY'),
    (4430, 'V', 1476, 2, '_CARRIER_COPY'),
    (4444, 'V', 688, 20, '_CARRIER_COPY'),
    (4458, 'V', 576, 20, '_CARRIER_COPY'),
    (4472, 'V', 1440, 20, '_CARRIER_COPY'),
    (4486, 'V', 1420, 20, '_CARRIER_COPY'),
    (4500, 'V', 1400, 20, '_CARRIER_COPY'),
    (4514, 'V', 1380, 20, '_CARRIER_COPY'),
    (4524, 'V', 1360, 20, '_CARRIER_COPY'),
    (4558, 'V', 436, 16, '_FINAL_LOCAL'),
    (4564, 'V', 452, 2, '_FINAL_LOCAL'),
    (4574, 'V', 454, 1, '_FINAL_LOCAL'),
    (4582, 'V', 416, 20, '_FINAL_LOCAL'),
    (4594, 'V', 396, 20, '_FINAL_LOCAL'),
    (4606, 'V', 376, 20, '_FINAL_LOCAL'),
    (4618, 'V', 356, 20, '_FINAL_LOCAL'),
    (4630, 'V', 336, 20, '_FINAL_LOCAL'),
    (4642, 'V', 316, 20, '_FINAL_LOCAL'),
    (4654, 'V', 296, 20, '_FINAL_LOCAL'),
    (4664, 'B', 34, 1, 'source_Config_split_byte'),
    (4670, 'B', 32, 2, 'source_Config_tag'),
    (4682, 'B', 54, 2, 'source_Config_tag'),
    (4692, 'B', 56, 20, '_COMMIT_inline'),
    (4700, 'B', 76, 2, 'source_Config_tag'),
    (4714, 'B', 98, 2, 'source_Config_tag'),
    (4726, 'B', 100, 20, '_COMMIT_inline'),
    (4734, 'B', 120, 2, 'source_Config_tag'),
    (4748, 'B', 142, 2, 'source_Config_tag'),
    (4760, 'B', 144, 20, '_COMMIT_inline'),
    (4768, 'B', 164, 2, 'source_Config_tag'),
    (4782, 'B', 186, 2, 'source_Config_tag'),
    (4794, 'B', 188, 20, '_COMMIT_inline'),
    (4812, 'V', 1000, 4, 'Q_header_or_count'),
    (4844, 'V', 1004, 4, 'Q_header_or_count'),
    (4918, 'V', 276, 4, 'source_tag_capture'),
    (4956, 'V', 272, 4, 'source_tag_capture'),
    (4994, 'V', 268, 4, 'source_tag_capture'),
    (5040, 'B', 208, 4, 'Warning_Config'),
    (5058, 'V', 292, 4, 'Q_header_or_count'),
    (5102, 'V', 1000, 8, 'Q_header_or_count'),
    (5166, 'V', 264, 4, 'source_tag_capture'),
    (5220, 'V', 1064, 2, 'Warning_scratch'),
    (5224, 'V', 1066, 1, 'Warning_scratch'),
    (5266, 'V', 260, 4, 'source_tag_capture'),
    (5304, 'V', 256, 4, 'source_tag_capture'),
    (5344, 'V', 252, 4, 'source_tag_capture'),
    (5376, 'V', 204, 4, 'pointer_spill'),
    (5382, 'V', 208, 4, 'pointer_spill'),
    (5398, 'V', 212, 4, 'pointer_spill'),
    (5408, 'V', 200, 4, 'pointer_spill'),
    (5426, 'V', 220, 4, 'source0_split_capture'),
    (5468, 'V', 204, 4, 'pointer_spill'),
    (5480, 'V', 208, 4, 'pointer_spill'),
    (5532, 'B', 29, 1, 'Fuel_Config'),
    (5534, 'B', 7, 1, 'Fuel_Config'),
    (5542, 'B', 28, 1, 'Fuel_Config'),
    (5546, 'B', 8, 4, 'Fuel_Config'),
    (5550, 'B', 12, 4, 'Fuel_Config'),
    (5554, 'B', 16, 4, 'Fuel_Config'),
    (5558, 'B', 20, 4, 'Fuel_Config'),
    (5562, 'B', 24, 4, 'Fuel_Config'),
    (5566, 'B', 4, 1, 'Fuel_Config'),
    (5568, 'B', 0, 4, 'Fuel_Config'),
    (5570, 'B', 5, 2, 'Fuel_Config'),
    (5618, 'B', 14526, 2, 'WarningColors_Config'),
    (5628, 'B', 14524, 2, 'WarningColors_Config'),
    (5736, 'V', 1000, 8, 'Q_header_or_count'),
    (5768, 'V', 1340, 16, '_TRANSPOSE'),
    (5772, 'V', 1356, 2, '_TRANSPOSE'),
    (5776, 'V', 1358, 1, '_TRANSPOSE'),
    (5786, 'V', 1320, 20, '_TRANSPOSE'),
    (5796, 'V', 1300, 20, '_TRANSPOSE'),
    (5808, 'V', 1280, 20, '_TRANSPOSE'),
    (5820, 'V', 1260, 20, '_TRANSPOSE'),
    (5830, 'V', 1240, 20, '_TRANSPOSE'),
    (5840, 'V', 1220, 20, '_TRANSPOSE'),
    (5852, 'V', 1200, 20, '_TRANSPOSE'),
    (6230, 'V', 1004, 4, 'Q_header_or_count'),
)

# Producer, cell, actual V-relative pointer, old/new consumer instruction.
POINTER_EPOCHS=((5376,204,630,5452),(5382,208,746,5462),
    (5398,212,1002,5504),(5408,200,804,5842),
    (5468,204,600,5798),(5480,208,488,5810))


def _parent_envelopes(objects,rows,parents,sequence,owner,protected):
    from config_decoded_sources import _parents
    require(type(sequence) is SequenceInterface and sequence.header.offset==288 and
        sequence.header.storage.extent==1608 and owner.space=='address' and owner.extent==14604 and
        owner.writable and owner.phase==sequence.header.storage.phase,
        'Sources parent envelope loses its typed current owner/frame')
    frame=sequence.header.storage;phase=frame.phase;V=parents[0]['function']-4376
    require(parents==_parents(objects,rows,V,owner.start),'Sources parent envelopes use another current family')
    require(sequence.decoder.header.storage.start==frame.start+1608 and
        sequence.decoder.header.storage.extent==456 and sequence.decoder.header.offset==236,
        'Sources parent envelopes lose the same live D/header')
    require_disjoint((Storage('address','reserved Sources descendants and callers',frame.start-120,120+1608+456,8,phase,False),owner))
    actual={p['function']-V+off for p in parents for off,width,text in GRAMMARS['visitor_'+str(p['function']-V)]['instructions']
        if text.split(' ',1)[0].split('.',1)[0] in ('str','strh','strb','strd','stm','vstr')}
    require(len(PARENT_WRITES)==80 and len({v[0] for v in PARENT_WRITES})==80 and
        actual=={v[0] for v in PARENT_WRITES},'Sources parent store partition is incomplete or duplicated')
    guards=list(protected)
    guards.extend((_object(sequence.decoder.header,'stable parent D'),
        _object(sequence.decoder.payload,'immutable parent input'),
        _object(borrow(frame,288,4,phase),'stable physical top Q.D'),
        _object(borrow(frame,1500,8,phase),'stable parent Source Q')))
    for g in tuple(guards):
        for space in ('address',frame.space):
            if g.space!=space and not any(v.space==space and v.start<=g.start and g.start+g.extent<=v.start+v.extent for v in guards):
                guards.append(Storage(space,'physical '+g.identity,g.start,g.extent,1,phase,False))
    guards=tuple(guards);writes=[]
    for site,coordinate,offset,size,category in PARENT_WRITES:
        require(coordinate in ('V','B'),'unreviewed Sources write coordinate')
        if coordinate=='V':
            require(0<=offset<offset+size<=1512,'Sources local write escapes its owned locals')
            target=borrow(frame,offset,size,phase,True)
        else:
            require(0<=offset<offset+size<=30 or 32<=offset<offset+size<=212 or
                14524<=offset<offset+size<=14528,'Sources write touches inactive padding or a later Config group')
            target=borrow(owner,offset,size,phase,True)
        operation=rows[V+site].mnemonic.split('.',1)[0]
        if operation in ('str','strd','stm','vstr'):
            require(target.start%4==0,'Sources parent multiple/word/FP store lacks actual alignment4')
        elif operation=='strh':
            require(target.start%2==0 or (site,coordinate,offset,size)==(5570,'B',5,2),
                'Sources parent adds an unreviewed unaligned halfword')
        require_writes((target,),((target,0,size),),guards)
        writes.append((V+site,target,category))
    require(sum(v[3] for v in PARENT_WRITES)==707,'Sources physical footprint partition changed')
    return dict(stores=80,local_stores=sum(v[1]=='V' for v in PARENT_WRITES),
        Config_stores=sum(v[1]=='B' for v in PARENT_WRITES),static_write_bytes=707,
        writes=tuple(writes),pointer_epochs=POINTER_EPOCHS,
        unaligned_parent_requirement=(V+5570,5,2),
        confers_active_state=False,requires_actual_source_control_pointer_epochs_and_child_effects=True)
