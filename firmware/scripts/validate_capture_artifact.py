"""Independently consume completed capture identity and strict converted UF2 bytes.
This proves a conditional software artifact, not hardware qualification.
"""
import hashlib,json,sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
from validation import require
from elf_image import ElfImage
from check_program_image import inspect_program,linked_configuration,linked_startup,read_uf2,verify_uf2
from check_memory_layout import read_image_result
from config_build_source import load_source_context

def bounded(path,maximum):
    with Path(path).open('rb') as stream:data=stream.read(maximum+1)
    require(len(data)<=maximum,'artifact input byte bound')
    return data

def unique(pairs):
    result={}
    for key,value in pairs:
        require(key not in result,'duplicate artifact key');result[key]=value
    return result

def record(path,maximum):return json.loads(bounded(path,maximum),object_pairs_hook=unique)
def sha(data):return hashlib.sha256(data).hexdigest()

def validate(capture,inputs,uf2):
    capture=Path(capture);inputs=Path(inputs)
    manifest=record(capture/'manifest.json',4*1024*1024)
    require(manifest.get('schema')==2,'completed schema2 capture required')
    elf=ElfImage(capture/'pico2.elf')
    require(sha(elf.data)==manifest['elf_sha256'],'capture ELF identity mismatch')
    eav=bounded(inputs/'dashboard.eav',65536)
    require(eav==bounded(capture/'dashboard.eav',65536),'accepted EAV differs from capture')
    require(manifest['dashboard_config']['embedded_sha256']==sha(eav),'capture configuration hash mismatch')
    options=record(inputs/'options.json',65536)
    attempt=bounded(capture/'build-options.json',65536)
    require(manifest['build_options']==json.loads(attempt,object_pairs_hook=unique) and manifest['build_options']['normalized']==options and manifest['build_options_sha256']==sha(attempt),'capture options mismatch')
    context_path=capture/'source-context.json'
    context=load_source_context(context_path,elf,capture.name,attempt)
    require(manifest['source_context']['sha256']==sha(bounded(context_path,262144)),'source context identity mismatch')
    memory_raw=bounded(capture/'memory-layout.json',65536)
    memory=read_image_result(json.loads(memory_raw,object_pairs_hook=unique),elf,eav,context)
    require(manifest['memory_layout']==memory and manifest['memory_layout_json_sha256']==sha(memory_raw),'full image evidence mismatch')
    mapping,program=inspect_program(elf)
    program['linked_configuration']=linked_configuration(elf,eav)
    profile=bounded(inputs/'startup.easp',16);asset=bounded(inputs/'startup.easa',524288)
    program['linked_startup']=linked_startup(elf,profile,asset)
    require(program==manifest['program_image'],'program image identity mismatch')
    converted=read_uf2(uf2);comparison=verify_uf2(converted,mapping)
    return {'schema_version':1,'scope':'conditional_software_artifact','hardware_qualified':False,'flash_ready':False,'elf_sha256':sha(elf.data),'uf2_sha256':sha(converted),'uf2_bytes':len(converted),'config_sha256':sha(eav),'startup_profile_sha256':sha(profile),'startup_asset_sha256':sha(asset),'options':options,'program_image':program,'uf2':comparison,'memory_layout':memory}

def main():
    import argparse
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('capture');parser.add_argument('inputs');parser.add_argument('uf2');args=parser.parse_args()
    result=validate(args.capture,args.inputs,args.uf2)
    print(json.dumps(result,sort_keys=True,separators=(',',':')))
if __name__=='__main__':main()
