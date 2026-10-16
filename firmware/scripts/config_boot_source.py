"""Narrow source semantics for the private Config and synchronous Boot body.

This is NOT build provenance or ownership. Its input must come from the same
checked build-source/ELF path before a history producer can consume it. The
reader never falls back to current checkout files. Exact machine callers,
source/private alias, first-batch state, diagnostics and loader effects remain
separate obligations. Source hashes below record identity, not approval.

Only the two authored Rust fragments are recognized. This is not a Rust
parser, macro expander or a general source call-graph/visibility evaluator.
Changed declarations, bodies, attributes and extra private-module items need
review; comments/whitespace alone do not alter the selected source contract.
"""
import hashlib
import re
from validation import require

_PRIVATE = """
use dashboard_pico2::view_config::{BootConfig, ConfigSlot};
use static_cell::ConstStaticCell;
static CONFIGURATION: ConstStaticCell<ConfigSlot> = ConstStaticCell::new(ConfigSlot::new());
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".dashboard_config")]
static DASHBOARD_EMBEDDED_CONFIG: [u8; include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.eav")).len()] =
    *include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.eav"));
pub const SHA256: &str = env!("DASHBOARD_CONFIG_SHA256");
#[inline(never)]
pub fn initialize() -> &'static BootConfig { CONFIGURATION.take().load_at_boot(&DASHBOARD_EMBEDDED_CONFIG) }
"""
_BOOT = """
async fn main(spawner: Spawner) {
    let configuration = view_config::initialize();
    spawner.spawn(dashboard_task(spawner, configuration).unwrap());
}
"""
_ATTRIBUTE=('#','[','embassy_executor','::','main',']')
_TOKEN=re.compile(r'[A-Za-z_][A-Za-z0-9_]*|[0-9]+|::|->|[^\s]')
_LIFETIME=re.compile(r"'[A-Za-z_][A-Za-z0-9_]*")


def _cooked_string(text,start):
    # The authored fragments need ordinary strings and cooked C strings,
    # with only these simple escapes. Preserve a whole literal as one token;
    # unsupported escapes and unterminated forms fail before matching names.
    at=start+1
    while at<len(text):
        if text[at]=='"':return at+1
        if text[at]=='\\':
            require(at+1<len(text) and text[at+1] in "nrt0\\'\"",'unsupported or incomplete cooked-string escape')
            at+=2
        else:at+=1
    require(False,'unterminated cooked source string')


def _tokens(raw):
    require(type(raw) is bytes and raw and len(raw)<=65536,'missing or oversized bounded source member')
    try:text=raw.decode('utf-8')
    except UnicodeDecodeError:raise AssertionError('source member is not UTF-8') from None
    require(not any(ord(c)<32 and c not in '\r\n\t' for c in text),'source contains an unsupported control byte')
    # Sequential consumption makes comment/literal contents indivisible.
    # Raw r/br/cr strings, raw identifiers, character/byte-character literals
    # and block comments are unsupported by this finite fragment contract.
    # Reject them at their introducer, before content can become declarations.
    tokens=[];at=0
    while at<len(text):
        if text[at].isspace():at+=1;continue
        if text.startswith('//',at):
            end=text.find('\n',at);at=len(text) if end<0 else end+1;continue
        require(not text.startswith(('/*','*/'),at),'block comments require an explicitly reviewed source lexer')
        if text[at]=='"':
            end=_cooked_string(text,at);tokens.append(text[at:end]);at=end;continue
        if text[at]=="'":
            lifetime=_LIFETIME.match(text,at)
            require(lifetime is not None and (lifetime.end()==len(text) or text[lifetime.end()]!="'"),
                'character literals or malformed lifetimes are unsupported')
            tokens.append(lifetime[0]);at=lifetime.end();continue
        token=_TOKEN.match(text,at);require(token is not None,'unsupported source token')
        value=token[0];end=token.end()
        require(not (value in ('r','br','cr') and end<len(text) and text[end] in '#"'),
            'raw strings/identifiers require an explicitly reviewed source lexer')
        require(not (value.isidentifier() and end<len(text) and text[end]=='"' and value not in ('b','c')),
            'unsupported cooked-string prefix')
        tokens.append(value);at=end
    return tuple(tokens)


def _occurrences(tokens,part):
    return tuple(i for i in range(len(tokens)-len(part)+1) if tokens[i:i+len(part)]==part)


def _top_level(tokens,index):
    stack=[];close={')':'(',']':'[','}':'{'}
    for token in tokens[:index]:
        if token in ('(','[','{'):stack.append(token)
        elif token in close:
            require(stack and stack.pop()==close[token],'source prefix has an unbalanced delimiter')
    return not stack


def _top_declaration(tokens,text):
    part=_tokens(text.encode());indices=_occurrences(tokens,part)
    require(len(indices)==1,'source declaration is missing or ambiguous')
    index=indices[0]
    require(_top_level(tokens,index) and (index==0 or tokens[index-1] in (';','}')),
        'source declaration is nested or has an extra visibility/attribute/alias prefix')
    return index


def first_boot_source_contract(documents):
    """Recognize source semantics only; no current-checkout or ELF fallback.

    A caller dictionary containing these bytes is not build-source evidence.
    The final producer must obtain the members through the checked immutable
    attempt/ELF input path and recheck that binding before issuing any history.
    """
    require(type(documents) is dict and all(type(name) is str for name in documents) and
        set(documents)=={'main.rs','view_config.rs'},
        'first-boot source members are missing or include an undeclared input')
    private=_tokens(documents['view_config.rs']);main=_tokens(documents['main.rs'])
    require(private==_tokens(_PRIVATE.encode()),'private Config module changes declaration/take/escape/return semantics')
    _top_declaration(main,'mod view_config;')
    _top_declaration(main,'use embassy_executor::{Executor, Spawner};')
    require(len(_occurrences(main,('fn','main')))==1,'Boot source has another main definition')
    indices=_occurrences(main,('async','fn','main','('))
    require(len(indices)==1,'Boot source has no unique synchronous generated main')
    index=indices[0];begin=index-len(_ATTRIBUTE)
    require(begin>=0 and _top_level(main,begin) and main[begin:index]==_ATTRIBUTE and (begin==0 or main[begin-1] not in (']','pub')),
        'Boot main attributes differ or have an additional conditional prefix')
    expected=_tokens(_BOOT.encode())
    require(main[index:index+len(expected)]==expected,'Boot main adds an escape/await/call or changes initialization order')
    require(index+len(expected)==len(main) or main[index+len(expected)] in ('#','async','fn','pub','use','mod','const','static','unsafe','impl','struct','enum'),
        'Boot source function has an unreviewed suffix')
    return dict(kind='first_boot_source_semantics_only',
        source_members={name:hashlib.sha256(raw).hexdigest() for name,raw in documents.items()},
        Config_visibility='private_static_single_take_immutable_return',
        ordered_boot_operations=('initialize','enqueue_Dashboard'),
        Boot_main_attribute='embassy_executor::main_default_pool',
        Config_source_expression='ConstStaticCell::new(ConfigSlot::new())',
        take_source_expression='CONFIGURATION.take().load_at_boot(&DASHBOARD_EMBEDDED_CONFIG)',
        requires_checked_build_source_ELF_binding=True,
        requires_actual_machine_and_first_batch_composition=True,
        requires_all_direct_computed_indirect_and_address_taken_reference_closure=True)
