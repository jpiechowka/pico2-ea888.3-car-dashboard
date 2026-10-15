"""Initial Config load-image specification, deliberately not live ownership.

The fixed mask admits only ConfigSlot::new/empty_config's active values.
It does not read unused vector elements or inactive enum payloads. Startup
copy/preservation, the successful exclusive claim and source visibility must
be composed separately before any acquired mutable Config can be issued.
"""
import hashlib
from config_entry import CONFIG,static_role
from config_initial_contract import GROUPS,OPAQUE
from config_logger import _initialized
from validation import require


def _partition():
    active={};names=[]
    require(type(GROUPS) is tuple and len(GROUPS)==15 and type(OPAQUE) is tuple,
        'initial contract loses a fixed group/opaque partition')
    for name,base,count,stride,fields in GROUPS:
        require(isinstance(name,str) and name not in names and all(type(x) is int for x in (base,count,stride)) and
            0<=base<14604 and 1<=count<=8 and 0<=stride<=14604 and type(fields) is tuple and fields,
            'malformed fixed initial-value group')
        names.append(name)
        for i in range(count):
            for offset,encoded in fields:
                require(type(offset) is int and offset>=0 and isinstance(encoded,str),'malformed active initial field')
                raw=bytes.fromhex(encoded);lo=base+i*stride+offset
                require(raw and 0<=lo<lo+len(raw)<=14604,'active initial field exceeds owner')
                for j,value in enumerate(raw):
                    require(lo+j not in active,'active initial fields overlap');active[lo+j]=value
    opaque=set();previous=0
    for lo,hi in OPAQUE:
        require(type(lo) is int and type(hi) is int and previous<=lo<hi<=14604,'malformed initial opaque interval')
        require(not any(i in active for i in range(lo,hi)),'opaque initial bytes hide an active value')
        opaque.update(range(lo,hi));previous=hi
    require(len(active)==185 and len(opaque)==14419 and set(active)|opaque==set(range(14604)),
        'initial active/opaque partition is incomplete')
    return active,tuple(names)


def initial_configuration(objects):
    """Compare the unique typed object's initial file bytes, never live RAM.

    No expected values, mask or caller-owned phase can be supplied. The return
    value intentionally grants no acquired/published/validated capability and
    cannot authorize reuse of this pre-write state after decode or refill.
    """
    entries=[v for v in objects.elf.symbol_entries if static_role(v[0])==CONFIG]
    require(len(entries)==1,'initial Config has no unique typed symbol')
    name,address,extent,info,index=entries[0]
    require(all(type(x) is int for x in (address,extent,info,index)) and 0<address<address+14604<=0x100000000 and
        address%4==0,'initial Config address/metadata/alignment differs')
    raw=_initialized(objects,address,14604,CONFIG);section=objects.elf.sections[index]
    owner_file=section[4]+address-section[3];loads=[]
    require(hasattr(objects.elf,'segments'),'initial Config lacks current load segments')
    for segment in objects.elf.segments:
        require(len(segment)==8 and all(type(x) is int for x in segment),'malformed initial load segment')
        kind,offset,virtual,physical,filesz,memsz,flags,alignment=segment
        if kind!=1 or not memsz or not (virtual<address+14604 and address<virtual+memsz):continue
        require(0<=virtual<virtual+memsz<=0x100000000 and 0<=filesz<=memsz and
            0<=physical<=physical+filesz<=0x100000000 and 0<=offset<=offset+filesz<=len(objects.elf.data),
            'initial load segment extent is truncated or wraps')
        loads.append(segment)
    require(len(loads)==1,'initial Config has missing or ambiguous load backing')
    kind,offset,virtual,physical,filesz,memsz,flags,alignment=loads[0]
    require(virtual<=address<address+14604<=virtual+filesz and offset+address-virtual==owner_file and flags&7==7,
        'initial Config section/load mapping differs')
    active,names=_partition()
    require(all(raw[offset]==value for offset,value in active.items()),'initial Config active constructor value differs')
    return dict(address=address,extent=14604,alignment=4,role=CONFIG,
        elf_sha256=hashlib.sha256(objects.elf.data).hexdigest(),file_offset=owner_file,
        load_physical=physical+address-virtual,phase='initial_load_image_only',groups=names,
        active_bytes=185,opaque_bytes=14419,opaque_intervals=OPAQUE,initial_collection_lengths=(0,0),
        requires_actual_startup_copy_and_live_preservation=True,
        requires_actual_successful_once_and_exclusive_source_owner=True,
        requires_first_batch_caller_specialized_loader_and_alias_closure=True,
        must_not_reuse_after_any_decode_or_refill_write=True,
        requires_complete_validated_outcome_before_publication=True)
