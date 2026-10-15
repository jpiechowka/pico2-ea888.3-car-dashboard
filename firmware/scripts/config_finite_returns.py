"""Shared closed child save/return equations for matched parser families.

Only fixed authored stack profiles supplied by the two internal consumers are
used. No scalar opcode is executed, no unknown PC effect is admitted, and
every complete body must already have its current literal contract checked.
This does not prove caller source effects, observer compatibility or ownership.
"""
import re
from config_regions import _control
from config_owner_entry import _direct_target
from validation import require


def _closed_child_returns(rows,parents,children,sp,stacks,scratch_restores):
    completed={};nodes=[];active=[]
    def walk(proof,entry,token,expected_sp,cells=(),lr=None,shuttle=None):
        start=proof['function'];family=proof.get('semantic_role',proof['role'])
        require(family in stacks,'finite child has no authored child return family')
        key=(start,entry,token,expected_sp,cells,lr,shuttle)
        if key in completed:return completed[key]
        require(start not in active,'finite child child introduces an activation cycle');active.append(start)
        body={a:r for a,r in rows.items() if start<=a<start+proof['extent']}
        edges={start+e[0]:e for e in proof['ordinary_edges']}
        require(not proof['indirect_sites'],'finite child child adds an executable callback')
        pending=[(start,entry,token if lr is None else lr,cells,shuttle)];seen=set();returns=[];minimum=entry
        while pending:
            at,sp,link,saved,reg2=pending.pop();state=(at,sp,link,saved,reg2)
            if state in seen:continue
            seen.add(state);require(at in body,'finite child child successor enters data or outside its body')
            row=body[at];op=row.mnemonic.removesuffix('.w');following=at+len(row.raw)
            control,conditional=_control(row);memory=dict(saved);new_sp=sp;new_link=link;return_pc=None
            stack_step=op.startswith(('push','pop')) or row.operands.startswith('sp,') or bool(re.fullmatch(r'r(?:8|11), \[sp(?:, #-4\]!|\], #4)',row.operands))
            require(not stack_step or at-start in stacks[family],'finite child child changes an unreviewed SP/save instruction')
            if at-start in stacks[family]:
                if op.startswith(('push','pop')):
                    regs=row.operands[1:-1].split(', ')
                    if op=='push':
                        new_sp=sp-4*len(regs)
                        for i,reg in enumerate(regs):
                            cell=new_sp+4*i;require(cell not in memory,'finite child nested save overwrites a live caller slot')
                            memory[cell]=link if reg=='lr' else reg
                    else:
                        for i,reg in enumerate(regs):
                            cell=sp+4*i;require(cell in memory,'finite child restore consumes an unowned save slot');value=memory.pop(cell)
                            if reg=='pc':return_pc=value
                            elif reg=='lr':new_link=value
                            elif reg in ('r2','r3'):
                                require(family in scratch_restores,'finite child restores scratch into another register family')
                            else:require(value==reg,'finite child restore consumes another caller register')
                        new_sp=sp+4*len(regs)
                elif op in ('add','sub'):
                    count=int(row.operands.split('#')[1],0);new_sp=sp+(count if op=='add' else -count)
                elif op=='str':
                    new_sp=sp-4;require(new_sp not in memory,'finite child high save overwrites its caller');memory[new_sp]=row.operands.split(',')[0]
                else:
                    require(op=='ldr' and memory.get(sp)==row.operands.split(',')[0],'finite child postindexed restore loses its high register')
                    del memory[sp];new_sp=sp+4
            if family=='slice_pop' and at-start==30:reg2=link
            elif family=='slice_pop' and at-start==36:new_link=reg2
            elif row.operands.startswith('lr,') and not stack_step:new_link=('scratch',at)
            minimum=min(minimum,new_sp);updated=tuple(sorted(memory.items()))
            if at in edges:
                off,operation,target,role,frame=edges[at]
                require(control in ('bl','b') and not conditional,'finite child child edge changes call/tail family')
                child=children[target,role]
                child_token=following if control=='bl' else token
                expected=new_sp if control=='bl' else expected_sp
                result=walk(child,new_sp,child_token,expected,updated,child_token if control=='bl' else new_link,reg2)
                minimum=min(minimum,result['lowest_SP'])
                if control=='b':returns.extend(result['returns']);continue
                require(result['restored_cells']==updated,'finite child child changes live ancestor save cells')
                new_link=following
            elif return_pc is not None or control=='bx':
                require(control=='return' if return_pc is not None else row.operands=='lr','finite child child has an unreviewed return consumer')
                pc=return_pc if return_pc is not None else new_link
                require(pc==token and new_sp==expected_sp,'finite child child loses its saved return PC or caller SP')
                returns.append((at,pc,new_sp))
                if conditional:
                    require(following in body,'conditional child restore falls outside its body')
                    pending.append((following,sp,link,saved,reg2))
                continue
            elif control in ('b','cbz','cbnz'):
                target=_direct_target(row,control);require(target in body,'finite child child local branch escapes its matched body')
                pending.append((target,new_sp,new_link,updated,reg2))
                if not conditional:continue
            else:require(control is None,'finite child child contains an unclassified PC effect')
            require(following in body,'finite child child falls outside its complete body')
            pending.append((following,new_sp,new_link,updated,reg2))
        require(returns,'finite child child has no completed saved-PC return')
        # A tail restore can consume its parent's cells. Its final map is the
        # caller's enclosing map, not necessarily the cells present at entry.
        final_cells=tuple((a,v) for a,v in cells if a>=expected_sp)
        result=dict(function=start,entry_SP=entry,return_PC=token,return_SP=expected_sp,
            lowest_SP=minimum,returns=tuple(sorted(set(returns))),restored_cells=final_cells,
            architectural_LR_preserved=False)
        nodes.append(result);completed[key]=result;active.pop();return result
    for parent in parents:
        for off,op,target,role,frame in parent['ordinary_edges']:
            require(op=='bl','Views parent changes its call phase')
            walk(children[target,role],sp,parent['function']+off+4,sp)
    return tuple(nodes)
