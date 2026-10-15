"""Finite conditional platform scope for named RP2350 stack accounting.

Version1 assumes a usable stack, normal fresh Secure Core0 IMAGE_DEF context,
Secure UNALIGN_TRP=0 at relevant loads, and readable Normal embedded input.
These are external conditions, never host-established hardware facts. They do
not waive source/body/ordinary/control/range/ownership/callback proof. Unknown
effects cannot become assumptions. No established policy is supported here.
The reviewed scope declaration is copied into these bounded constants so a
fresh checkout does not depend on an ignored report or runtime approve switch.
"""
from dataclasses import dataclass,asdict
import hashlib
import re
import struct
import zlib
from config_regions import View
from validation import require

TARGET='thumbv8m.main-none-eabihf'
BOOT_CONTEXT='fresh_normal_core0_image_def'
SCOPE='h2-rp2350-secure-embedded-eav-v1'
SCOPE_SHA256='f280d95716938fd66da3ee3a716acf604d35fec9b75c24807230ef3149bba69a'
ASSUMPTIONS=(
    'A valid usable stack is available.',
    'Executing Core0 is Secure in the normal fresh ROM IMAGE_DEF context; other entry modes are outside scope.',
    'Secure CCR.UNALIGN_TRP remains zero throughout the relevant f32 single-LDR accesses.',
    'The complete bound embedded EAV payload remains readable Normal memory under effective MPU, security and XIP mapping.',
)
BOUND_KEYS=('boot','constructor','runtime','dashboard')


@dataclass(frozen=True)
class PlatformPolicy:
    input_start:int
    input_end:int
    input_sha256:str
    elf_sha256:str
    schema:int=1
    scope:str=SCOPE
    scope_sha256:str=SCOPE_SHA256
    status:str='assumed_external'
    target:str=TARGET
    core:int=0
    security:str='secure'
    boot_context:str=BOOT_CONTEXT
    input_role:str='embedded_eav_payload'
    assumptions:tuple=ASSUMPTIONS
    hardware_qualified:bool=False

    def __post_init__(self):
        require(type(self.schema) is int and self.schema==1 and (self.scope,self.scope_sha256)==(SCOPE,SCOPE_SHA256),
                'missing/stale platform scope declaration')
        require(self.status=='assumed_external' and self.hardware_qualified is False,'unresolved or unproved platform status')
        require(self.target==TARGET and type(self.core) is int and self.core==0 and self.security=='secure' and
                self.boot_context==BOOT_CONTEXT and self.input_role=='embedded_eav_payload','platform target/bank/input context differs')
        require(type(self.input_start) is int and type(self.input_end) is int and
                0x10000000<=self.input_start<self.input_end<=0x101fe000,'platform input exceeds program XIP range')
        require(all(isinstance(v,str) and re.fullmatch('[0-9a-f]{64}',v) for v in (self.input_sha256,self.elf_sha256)),
                'platform input/artifact identity is missing')
        require(self.assumptions==ASSUMPTIONS and isinstance(self.assumptions,tuple),'platform assumptions changed or lost')

    def for_f32_load(self,cursor):
        require(type(cursor) is int and self.input_start<=cursor<cursor+4<=self.input_end,'f32 read exceeds bound platform input')
        return self

    def to_record(self):
        record=asdict(self);record['assumptions']=list(self.assumptions);return record


def reviewed_eav_policy(objects,payload,context,*,contradictory_effects):
    """Called only with recognized source input and closed source-effect facts.

    The returned assumption does not establish their constructor/transport or
    preservation. This factory has no caller-selected scope/status/approval.
    """
    require(isinstance(context,dict) and context==dict(target=TARGET,core=0,security='secure',boot_context=BOOT_CONTEXT) and
            type(context.get('core')) is int,'unreviewed platform entry context')
    require(isinstance(contradictory_effects,tuple) and not contradictory_effects,'known/unknown contradictory platform effect')
    require(isinstance(payload,View) and payload.storage.space=='address' and payload.storage.identity=='embedded_eav' and
            not payload.storage.writable and not payload.writable and payload.offset==12 and
            payload.extent==payload.storage.extent-12,'platform input lacks its recognized complete readonly EAV payload')
    raw=objects.read_only(payload.start,payload.extent)
    return PlatformPolicy(payload.start,payload.start+payload.extent,hashlib.sha256(raw).hexdigest(),hashlib.sha256(objects.elf.data).hexdigest())


def read_policy(record,expected):
    require(isinstance(expected,PlatformPolicy) and isinstance(record,dict),'absent/unresolved platform prerequisite')
    wanted=expected.to_record()
    require(set(record)==set(wanted) and type(record.get('schema')) is int and type(record.get('core')) is int and
            type(record.get('input_start')) is int and type(record.get('input_end')) is int and
            record.get('hardware_qualified') is False and record==wanted,'platform qualifier or current target/input identity differs')
    return expected


def compose_policy(expected,children):
    require(isinstance(expected,PlatformPolicy) and isinstance(children,(tuple,list)) and bool(children),'lost dependent platform prerequisite')
    require(all(isinstance(child,PlatformPolicy) and child==expected for child in children),'dependent platform prerequisite differs or was dropped')
    return expected


def qualified_stack_result(bounds,headroom,policy):
    """Common post-analysis boundary; callers still must close all source paths."""
    require(isinstance(policy,PlatformPolicy),'stack result has no resolved conditional scope')
    require(isinstance(bounds,dict) and set(bounds)==set(BOUND_KEYS) and
            all(type(v) is int and 0<=v<0x100000000 for v in bounds.values()) and
            type(headroom) is int and 0<headroom<=0x80000,'invalid stack result bounds/headroom')
    from check_stack_budget import budgets
    budgets(*(bounds[k] for k in BOUND_KEYS),headroom)
    return dict(schema=1,status='CONDITIONAL_STACK_PASS',platform_policy='assumed_external',hardware_qualified=False,
                stack_bounds=dict(bounds),headroom=headroom,policies=[policy.to_record()])


def read_stack_result(record,expected_policy,expected_headroom):
    require(isinstance(record,dict) and set(record)=={'schema','status','platform_policy','hardware_qualified','stack_bounds','headroom','policies'} and
            type(record['schema']) is int and record['schema']==1 and record['status']=='CONDITIONAL_STACK_PASS' and
            record['platform_policy']=='assumed_external' and record['hardware_qualified'] is False and
            type(record['headroom']) is int and type(expected_headroom) is int and record['headroom']==expected_headroom and
            isinstance(record['policies'],list) and len(record['policies'])==1,'stack consumer lost conditional policy/qualification')
    policy=read_policy(record['policies'][0],expected_policy)
    return qualified_stack_result(record['stack_bounds'],record['headroom'],policy)


def current_image_policy(record,elf,expected_eav=None):
    """Check an existing producer policy against current artifact/input bytes.

    This consumer does not discover the used source or establish graph proof.
    The source-reviewed stack producer must supply the actual policy; no scan
    or caller approval can create one here. Capture additionally binds the
    exact generated complete EAV, independently of its declared payload hash.
    """
    require(isinstance(record,dict) and all(k in record for k in
        ('input_start','input_end','input_sha256','elf_sha256')),'missing current image platform policy')
    policy=PlatformPolicy(record['input_start'],record['input_end'],record['input_sha256'],record['elf_sha256'])
    read_policy(record,policy)
    require(hashlib.sha256(elf.data).hexdigest()==policy.elf_sha256,'platform policy belongs to another ELF')
    from config_stack import DiagnosticObjects
    objects=DiagnosticObjects(elf,{})
    extent=policy.input_end-policy.input_start
    require(12<extent+12<=16384 and policy.input_start>=0x1000000c,'platform payload has no bounded EAV envelope')
    start=policy.input_start-12
    backing=[]
    for section in elf.sections:
        if not section[2]&2 or not section[5] or not (section[3]<policy.input_end and start<section[3]+section[5]):continue
        require(0<=section[3]<section[3]+section[5]<=0x100000000,'platform input allocation wraps')
        backing.append(section)
    require(len(backing)==1 and backing[0][1]==1 and not backing[0][2]&1 and
            backing[0][3]<=start<policy.input_end<=backing[0][3]+backing[0][5],
            'platform input has aliased or writable backing')
    raw=objects.read_only(policy.input_start-12,extent+12)
    require(raw[:4]==b'EAV1' and struct.unpack_from('<HH',raw,4)==(4,extent) and
            zlib.crc32(raw[:8]+raw[12:])==struct.unpack_from('<I',raw,8)[0],
            'platform payload does not match its current complete EAV envelope')
    require(hashlib.sha256(raw[12:]).hexdigest()==policy.input_sha256,'platform payload content changed')
    if expected_eav is not None:
        require(isinstance(expected_eav,bytes) and raw==expected_eav,'generated capture input differs from stack policy')
    return policy
