"""Invocation-local immutable representation of freshly checked copy bodies.

This does not cache copy calls or dynamic states. Only complete equal child
descriptions from the same detached provider/map may share after each existing
copy API succeeds. Unsupported providers/values keep the checked unshared path.
"""
from types import MappingProxyType
from config_structural import _FrozenObjects
from validation import require


def _immutable_description(value):
    memo={};active=set()
    def visit(item):
        kind=type(item)
        if kind in (str,bytes,int,bool,type(None)):return item
        if kind not in (dict,tuple):raise TypeError('unsupported immutable copy description')
        identity=id(item)
        if identity in active:raise TypeError('cyclic immutable copy description')
        if identity in memo:return memo[identity]
        active.add(identity)
        if kind is tuple:result=tuple(visit(v) for v in item)
        else:result=MappingProxyType({visit(k):visit(v) for k,v in item.items()})
        active.remove(identity);memo[identity]=result;return result
    return visit(value)


class _CopyDescriptions:
    def __init__(self,objects,rows):
        self.objects=objects;self.rows=rows;self._values={}

    def checked(self,effect,objects,rows,target):
        require(objects is self.objects and rows is self.rows,
            'Sources copy description changes its invocation input context')
        require(type(target) is int,'Sources copy description target is not a typed address')
        # Unsupported readers still execute every original copy/body/provider
        # check. They acquire no new structural reuse or frozen-input claim.
        if type(objects) is not _FrozenObjects:return effect
        require(rows is objects._rows,'Sources copy description changes its detached map')
        require(type(effect) is dict and type(effect.get('children')) is tuple,
            'Sources copy description is not a complete checked child tuple')
        from config_decoded_sources import _phase_graph
        try:
            frozen=_immutable_description(effect['children'])
        except TypeError:return effect
        # The complete typed values, internal alias edges, contextual target
        # and every additional supported field participate, never a digest.
        key=(target,_phase_graph(effect['children']))
        if key not in self._values:self._values[key]=frozen
        return dict(effect,children=self._values[key])
