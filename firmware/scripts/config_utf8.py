"""Source-bound abstract UTF8 Result12 for one reviewed complete core body.

No byte string is interpreted. The pinned validation grammar establishes the
read guards and complete valid/error union for a correlated immutable slice.
Caller initialization, frame/source history and whole graph remain required.
"""
from dataclasses import dataclass
from config_abstract import Span
from config_decode_effects import _object
from config_finite import _complete_body
from config_regions import Storage,View,require_disjoint,require_writes,Region,Transfer,audit_coverage,_control
from config_stack import canonical_symbol
from config_owner_entry import _direct_target
from validation import require
from config_structural import _structural
from config_views_contracts import GRAMMARS

UTF8='_RNvNtNtCs_4core3str8converts9from_utf8'
_WIDTHS=bytes([1]*128+[0]*66+[2]*30+[3]*16+[4]*5+[0]*11)


@dataclass(frozen=True)
class UTF8Slice:
    """Paired domain: start=payload+offset, 0<=length<=end-offset.

    A dependent bound, not the independent product of the two intervals.
    Every byte of the immutable enclosing allocation is initialized; no valid
    UTF8 fact follows until the checked child's successful alternative.
    """
    payload:View
    offset:Span
    length:Span

    def __post_init__(self):
        require(isinstance(self.payload,View) and self.payload.storage.space=='address' and
            not self.payload.writable and not self.payload.storage.writable and
            isinstance(self.offset,Span) and isinstance(self.length,Span) and self.payload.start>0 and
            self.offset.upper+self.length.lower<=self.payload.extent and
            self.offset.lower+self.length.upper<=self.payload.extent,
            'UTF8 lacks a bounded initialized nonnull paired slice')
        # Each offset is feasible with length.lower. The largest dependent
        # endpoint is clipped by the allocation, not the impossible product
        # corner offset.upper+length.upper. Empty one-past pointers also have
        # to fit the actual32-bit Result word without wrapping to null.
        require(0<self.payload.start+self.offset.lower<=self.payload.start+self.offset.upper<0x100000000 and
            self.payload.start+min(self.payload.extent,self.offset.upper+self.length.upper)<0x100000000,
            'UTF8 derived pointer or endpoint is not representable without wrapping')


def _widths(objects,address,descriptor):
    require(descriptor==dict(kind='utf8_widths',extent=256) and objects.read_only(address,256)==_WIDTHS,
        'UTF8 width table differs from the pinned source domains')
    return ()


def _no_child(*args):require(False,'unexpected UTF8 child/outline')


@_structural(1)
def utf8_body(objects,instructions):
    names=[n for n in objects.elf.functions if canonical_symbol(n)==UTF8]
    require(len(names)==1,'UTF8 has no unique current typed body');name=names[0]
    proof=_complete_body(objects,name,instructions,UTF8,'source_bound_utf8',GRAMMARS['source_bound_utf8'],_widths,_no_child)
    require(proof['frame']==24 and not proof['ordinary_edges'] and not proof['indirect_sites'],'UTF8 child/activation differs')
    start=proof['function'];context=(start,'UTF8 source validation');transfers=[]
    for off,width,text in GRAMMARS['source_bound_utf8']['instructions']:
        row=instructions[start+off];family,conditional=_control(row)
        if family in ('b','cbz','cbnz'):
            target=_direct_target(row,family);transfers.append(Transfer(row.address,row.mnemonic,
                'loop' if target<=row.address else 'branch',target,'bounded byte/word UTF8 source invariant'))
        elif family=='return':
            require(off==360 and not conditional,'UTF8 saved return differs')
            transfers.append(Transfer(row.address,row.mnemonic,'return',None,'restored original r8/r4-r7/PC'))
        else:require(family is None,'UTF8 contains an unclassified PC effect')
    region=Region(context,'helper',start,start+368,tuple(start+o for o,w,t in GRAMMARS['source_bound_utf8']['instructions']),(),tuple(transfers))
    coverage=audit_coverage(objects,name,instructions,context,(region,))
    return dict(proof,coverage=coverage)


def from_utf8_interface(objects,instructions,source,output,frame,ancestor_protected):
    proof=utf8_body(objects,instructions)
    require(isinstance(source,UTF8Slice) and isinstance(output,View) and output.writable and output.extent==12 and
        output.start%4==0 and output.storage.alignment>=4 and isinstance(frame,Storage) and frame.writable and
        frame.space.startswith('stack:') and frame.extent==24 and frame.alignment>=8 and
        output.storage.phase==source.payload.storage.phase==frame.phase,
        'UTF8 output/frame/source alignment or lifetime differs')
    guards=tuple(ancestor_protected);table=Storage('address','UTF8 width data',proof['objects'][0],256,1,frame.phase,False)
    source_owner=_object(source.payload,'complete immutable UTF8 input');out=_object(output,'UTF8 Result12')
    require_disjoint((frame,)+guards)
    # Physical intervals precede namespace distinctions. Supplied contexts do
    # not prove an actual stack allocation; the public parent derives/checks it.
    for left,right in ((frame,out),(frame,source_owner),(frame,table),(out,source_owner),(out,table)):
        require(left.start+left.extent<=right.start or right.start+right.extent<=left.start,
            'UTF8 physical input/output/frame/table overlap')
    for v in guards:
        for mutable in (frame,out):require(mutable.start+mutable.extent<=v.start or v.start+v.extent<=mutable.start,
            'UTF8 physical output/save ancestor overlap')
    require_writes((output,),((output,4,8),(output,0,4)),guards+(frame,source_owner,table))
    objects.read_only(source.payload.start,source.payload.extent)
    errors=[];maximum=source.length.upper
    if maximum:
        # Some1 includes an invalid lead byte; Some2/3 require the actual
        # subsequent invalid byte. None has1..3 trailing bytes, no active O9.
        for m in (1,2,3):
            minimum=1 if m==1 else m+1
            if maximum>=minimum:errors.append(dict(option_tag=1,error_length=m,valid_up_to=Span(0,maximum-minimum),
                length_guard=('n>=k+'+str(minimum)),active_output_offsets=(0,1,2,3,4,5,6,7,8,9)))
        errors.append(dict(option_tag=0,error_length=None,valid_up_to=Span(0,maximum-1),
            length_guard='1<=n-k<=3',active_output_offsets=(0,1,2,3,4,5,6,7,8)))
    return dict(proof=proof,success=dict(tag=0,source=source,encoding=('O0=0','O4=p','O8=n'),valid_UTF8_for_same_pair=True),
        errors=tuple(dict(v,tag=1,encoding='O0=1,O4=k,O8=option_tag|(active length<<8)',guard='0<=k<n') for v in errors),
        write_footprint=((output.start+4,8),(output.start,4)),possible_writes=((output,4,8),(output,0,4)),
        input_reads=dict(byte_offsets=(54,96,134,150,176,258,272,310),guard='i<n at each actual byte read',
            aligned_word_offsets=(80,82),word_guard='p+i aligned4 and i<max(n-7,0) implies i+8<=n',
            word_stride=8,nonascii_mask=0x80808080,width_index=(128,255),input_envelope=source.payload),
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        conservative_activation_frames=(24,),
        **{'return':dict(SP=frame.start+24,saved_PC=(frame.start+20,frame.start+24),site=proof['function']+360,
            original_r8=(frame.start,4),original_r4_to_r7=(frame.start+4,16),architectural_LR_not_preserved=True)},
        requires_actual_caller_slice_transport_return_token_and_source_ownership=True)
