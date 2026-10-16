"""Four bounded conditional Sources requirements, never reached-use authority.

Eight nonaffine output epochs stay one family. Reading or matching these
records proves neither source/variant selection nor initialized contents,
ordinary continuation, mutable ownership or runtime memory permissions.
"""
from dataclasses import dataclass,asdict
from config_regions import Storage,View,borrow,require_disjoint,require_writes
from config_owned_access import _storage,_view
from config_source_contracts import ROLES
from validation import require


EXPECTED_SOURCE_ROLES=('source_route_local','source_route_output','warning_presentation_local','Fuel_final_halfword')
SOURCE_EPOCHS=((4376,1128,284),(4906,516,276),(4944,628,272),(4982,744,268),
    (5154,880,264),(5254,940,260),(5292,1000,256),(5332,1064,252))
SITE_SHAPES={
 'source_route_local':((486,'bdf81910',1,2,'read','paired_Physical11_request_response'),),
 'source_route_output':((724,'c4f80600',6,4,'write','same_Route_payload_before_selector_and_Poll_high_byte'),),
 'warning_presentation_local':((5228,'bdf82904',1,2,'read','paired_page_scope_before_mode_pack'),),
 'Fuel_final_halfword':((5570,'a1f80500',5,2,'write','paired_Fuel_payload_after_emitted_tag'),),
}
_PHASE='decoded_sources_group'
_VISITOR='_RINvXs_NtCs_16dashboard_config15decode_in_placeNtB5_10ConfigSeedNtNtCs_10serde_core2de7Visitor9visit_seqINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB25_7flavors5SliceEEB7_'


@dataclass(frozen=True)
class SourceAccessContext:
    loader:Storage
    visitor:Storage
    frame:Storage
    config:View
    protected:tuple

    def __post_init__(self):
        require(all(isinstance(v,Storage) and v.space.startswith('stack:') and v.phase==_PHASE and
            v.writable and v.alignment>=8 for v in (self.loader,self.visitor,self.frame)),
            'Sources access loses its typed activation')
        require(self.loader.extent==456 and self.visitor.extent==1608 and
            self.visitor.start==self.loader.start-1608 and self.loader.space==self.visitor.space==self.frame.space and
            (self.frame==self.visitor or self.frame.extent==72 and self.frame.start==self.visitor.start-72),
            'Sources access changes loader/visitor/child geometry')
        require(isinstance(self.config,View) and not self.config.writable and self.config.offset==0 and
            self.config.extent==14576 and self.config.storage.extent==14604 and self.config.storage.writable and
            self.config.storage.space=='address' and self.config.storage.phase==_PHASE and
            self.config.storage.identity=='private unpublished ConfigSlot' and self.config.storage.alignment>=4 and
            0x20000000<=self.config.start<self.config.start+14604<=0x20080000,
            'Sources access loses the complete conditional Config allocation')
        require_disjoint((self.loader,self.visitor,self.config.storage))
        # These contexts use actual addresses, not relative stack coordinates.
        # Preserve the full possible deeper call envelope across space labels.
        low=self.visitor.start-120;high=self.loader.start+self.loader.extent
        require(0<=low<high<=0x100000000,'Sources physical activation wraps or has a relative address')
        physical=Storage('address','conditional Sources descendants and callers',low,high-low,1,_PHASE,False)
        require_disjoint((physical,self.config.storage))
        require(type(self.protected) is tuple and 1<=len(self.protected)<=16 and
            all(isinstance(v,Storage) and v.phase==_PHASE and not v.writable for v in self.protected),
            'Sources access loses bounded typed protection')
        require(len({(v.space,v.start,v.extent) for v in self.protected})==len(self.protected),
            'Sources access protection duplicates an interval')
        for at,size in ((self.loader.start+352,104),(self.visitor.start+1512,96)):
            require(sum(v.space==self.loader.space and v.start==at and v.extent==size for v in self.protected)==1,
                'Sources access omits a parent saved interval')
        require(any(v.space=='address' and v.start<=self.config.start+14576 and
            self.config.start+14604<=v.start+v.extent for v in self.protected),
            'Sources access drops metadata/taken/padding protection')


@dataclass(frozen=True)
class SourceFamily:
    role:str
    mode:str
    parent:int
    caller:int
    sites:tuple
    context:SourceAccessContext
    epochs:tuple

    def __post_init__(self):
        require(type(self.role) is str and self.role in SITE_SHAPES and self.mode=='sizez',
            'unreviewed Sources role/mode')
        source=self.role in EXPECTED_SOURCE_ROLES[:2];extent=754 if source else 7128
        require(type(self.parent) is int and self.parent%2==0 and
            0x10000000<=self.parent<self.parent+extent<=0x101fe000 and type(self.caller) is int and
            self.caller%2==0 and 0x10000000<=self.caller<self.caller+7128<=0x101fe000 and
            (source or self.parent==self.caller),'Sources access parent/caller leaves its current extent')
        require(type(self.sites) is tuple and len(self.sites)==1 and
            all(type(v) is tuple and len(v)==6 and type(v[0]) is int and type(v[1]) is str and
                type(v[2]) is int and type(v[3]) is int and type(v[4]) is str and type(v[5]) is str for v in self.sites) and
            tuple((at-self.parent,*rest) for at,*rest in self.sites)==SITE_SHAPES[self.role],
            'Sources access site/width/direction/source pairing differs')
        require(type(self.context) is SourceAccessContext,'Sources access has no typed context')
        c=self.context
        require(type(self.epochs) is tuple and all(type(v) is tuple and len(v)==4 and
            all(type(n) is int for n in v) for v in self.epochs),'malformed Sources epoch tuple')
        wanted=tuple((self.caller+off,out,tag,8-i) for i,(off,out,tag) in enumerate(SOURCE_EPOCHS)) if source else ()
        require(self.epochs==wanted,'Sources access loses its eight ordered call/output/tag/count pairs')
        if source:
            require(c.frame.extent==72 and c.frame.start+72==c.visitor.start,
                'Sources access loses its actual child frame')
            require_disjoint((c.frame,c.visitor,c.loader,c.config.storage))
            require(sum(v.space==c.frame.space and v.start==c.frame.start+36 and v.extent==36
                for v in c.protected)==1,'Sources access omits Source saved cells')
        else:require(c.frame==c.visitor,'Sources parent access borrows a child activation')
        for index in range(8 if source else 1):
            owner=self.owner(index)
            for at,raw,field,width,direction,label in self.sites:
                require(field+width<=owner.extent,'Sources access escapes its exact owner')
                if direction=='write':require_writes((owner,),((owner,field,width),),c.protected)
                else:
                    footprint=Storage(owner.storage.space,'exact Sources read',owner.start+field,width,1,_PHASE,False)
                    for guard in c.protected:require_disjoint((footprint,guard))

    def owner(self,index):
        require(type(index) is int and 0<=index<(8 if self.epochs else 1),'Sources access has a stale call epoch')
        c=self.context
        if self.role=='source_route_local':return borrow(c.frame,24,4,_PHASE)
        if self.role=='source_route_output':return borrow(c.visitor,self.epochs[index][1],22,_PHASE,True)
        if self.role=='warning_presentation_local':return borrow(c.visitor,1064,3,_PHASE)
        return borrow(c.config.storage,0,32,_PHASE,True)

    def footprints(self,index):
        owner=self.owner(index)
        return tuple((owner.start+field,owner.start+field+width) for at,raw,field,width,direction,label in self.sites)

    def to_record(self):
        c=self.context
        return dict(kind='decoded_source_access_family',role=self.role,mode=self.mode,parent=self.parent,caller=self.caller,
            sites=[dict(site=a,encoding=raw,offset=o,width=w,direction=d,source_phase=p) for a,raw,o,w,d,p in self.sites],
            context=dict(loader=asdict(c.loader),visitor=asdict(c.visitor),frame=asdict(c.frame),
                config=asdict(c.config),protected=[asdict(v) for v in c.protected]),epochs=[list(v) for v in self.epochs],
            source_phase='actual_Source_aggregate_still_requires_complete_producer',physical_tag_is_completed_Source=False)


def read_source_family(record):
    require(type(record) is dict and set(record)==set(SourceFamily.__dataclass_fields__)|
        {'kind','source_phase','physical_tag_is_completed_Source'} and record['kind']=='decoded_source_access_family' and
        type(record['sites']) is list and len(record['sites'])==1 and type(record['epochs']) is list and
        len(record['epochs']) in (0,8),'malformed Sources access family record')
    c=record['context']
    require(type(c) is dict and set(c)==set(SourceAccessContext.__dataclass_fields__) and
        type(c['protected']) is list and 1<=len(c['protected'])<=16,'malformed Sources access context')
    context=SourceAccessContext(_storage(c['loader']),_storage(c['visitor']),_storage(c['frame']),
        _view(c['config']),tuple(_storage(v) for v in c['protected']))
    sites=[]
    for v in record['sites']:
        require(type(v) is dict and set(v)=={'site','encoding','offset','width','direction','source_phase'},
            'malformed Sources access site')
        sites.append(tuple(v[k] for k in ('site','encoding','offset','width','direction','source_phase')))
    require(all(type(v) is list and len(v)==4 for v in record['epochs']),'malformed Sources access epoch')
    result=SourceFamily(record['role'],record['mode'],record['parent'],record['caller'],tuple(sites),context,
        tuple(tuple(v) for v in record['epochs']))
    require(record['physical_tag_is_completed_Source'] is False and record==result.to_record(),
        'Sources requirement loses its exact conditional/partial-state qualification')
    return result


def current_source_family(objects,value):
    """Current role/extent/mapping/four-site bytes only, never a reached use."""
    from config_entry import CONFIG,entry_object
    from config_stack import typed_function
    from llvm_stack import mapping_ranges
    require(type(value) is SourceFamily,'missing typed Sources family')
    source=value.role in EXPECTED_SOURCE_ROLES[:2]
    role,extent,frame=(ROLES['ChannelSource'],754,72) if source else (_VISITOR,7128,1608)
    actual,stack,name=typed_function(objects,value.parent,role)
    require((actual,stack)==(extent,frame),'Sources current parent extent/frame differs')
    caller_extent,caller_frame,_=typed_function(objects,value.caller,_VISITOR)
    require((caller_extent,caller_frame)==(7128,1608),'Sources current caller extent/frame differs')
    entry_object(objects,value.context.config.start,dict(kind='entry_static',role=CONFIG))
    mapping=mapping_ranges(objects.elf,name)
    for at,raw,offset,width,direction,label in value.sites:
        encoded=bytes.fromhex(raw)
        require(any(k=='t' and lo<=at<at+len(encoded)<=hi for lo,hi,k in mapping),
            'Sources operation is not wholly mapped Thumb code')
        require(objects.read_only(at,len(encoded))==encoded,'Sources operation bytes differ from current artifact')
    return value
