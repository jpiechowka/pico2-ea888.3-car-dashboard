"""Focused actual-input Sources continuation checks.

Fixtures retain their supplied machine/source attribution. The complete
producer, ownership and final resource result remain separate gates.
"""
import unittest
from dataclasses import replace


class SourcePrivateHandoff(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from test_config_stack import ActualDecodedWidgets
        from config_decoded_widgets import _first_boot_decoded_widgets_state
        objects,rows,source,_=ActualDecodedWidgets.fixture()
        _,cls.state=_first_boot_decoded_widgets_state(objects,rows,source)
        cls.objects=cls.state.family_bridge.objects;cls.rows=cls.state.family_bridge.rows

    def test_actual_zero_and_nonempty_Widget_exits_retain_carriers_and_prefix(self):
        from config_source_handoff import _source_entries
        entries=_source_entries(self.objects,self.rows,self.state)
        self.assertEqual(len(entries),len(self.state.exits))
        frame=self.state.views.context.headers['visitor_frame'];B=self.state.views.context.owner.start
        self.assertTrue(any(not e.exit.committed for e in entries))
        self.assertTrue(any(e.exit.committed for e in entries))
        for e in entries:
            self.assertTrue(any(e.exit is v for v in self.state.exits))
            self.assertEqual(e.sequence.count.lower,8)
            self.assertEqual(e.call_site,self.state.views.context.headers['visitor']['proof']['function']+4376)
            self.assertEqual(dict(e.call_arguments),{'r0':frame.start+1128,'r1':frame.start+1500})
            self.assertEqual((e.old_Source_array.start,e.old_Source_array.extent,e.old_Source_array.writable),(B+32,176,False))
            self.assertTrue(any(v.space=='address' and (v.start,v.extent)==(B+244,200) for v in e.protected))
            self.assertEqual(tuple((v.start-frame.start,target-frame.start) for v,target in e.carriers),
                ((216,1001),(224,688),(228,576),(232,1420),(236,1380),(240,1400),(244,1440),(248,1360)))
            if e.exit.committed:
                n=220*len(e.exit.committed)
                self.assertTrue(any(v.space=='address' and (v.start,v.extent)==(B+444,n) for v in e.protected))
                self.assertTrue(any(v.space==frame.space and (v.start,v.extent)==(B+444,n) for v in e.protected))
            self.assertFalse(e.completed_Source_issued)

    def test_copied_description_or_reordered_commit_is_not_a_selected_exit(self):
        from config_source_handoff import _source_entries
        from config_abstract import Span
        state=self.state
        changed=replace(state,exits=(replace(state.exits[0],sequence=replace(state.exits[0].sequence,count=Span(6,6))),))
        with self.assertRaisesRegex(AssertionError,'Sources predecessor'):
            _source_entries(self.objects,self.rows,changed)
        e=next(v for v in state.exits if v.committed)
        item=replace(e.committed[0],source_fields=(('fake','receipt'),))
        changed=replace(state,exits=(replace(e,committed=(item,)+e.committed[1:]),))
        with self.assertRaisesRegex(AssertionError,'source/commit'):
            _source_entries(self.objects,self.rows,changed)

    def test_current_carrier_instruction_and_dependent_metadata_cannot_drift(self):
        from config_source_handoff import _source_entries
        state=self.state;V=state.views.context.headers['visitor']['proof']['function']
        changed=dict(self.rows);row=changed[V+546]
        changed[V+546]=replace(row,operands='r0, [sp, #0xdc]')
        with self.assertRaises(AssertionError):
            _source_entries(self.objects,changed,state)
        empty=replace(state.exits[0],Source_sequence=replace(state.exits[0].Source_sequence,
            decoder=replace(state.exits[0].Source_sequence.decoder,cursor=state.exits[0].sequence.decoder.cursor)))
        # A source header copied to a different current epoch remains invalid
        # even if its pointer and count values appear equal.
        from config_regions import borrow
        frame=state.views.context.headers['visitor_frame']
        empty=replace(empty,Source_sequence=replace(empty.Source_sequence,header=borrow(frame,1496,8,frame.phase,True)))
        with self.assertRaisesRegex(AssertionError,'Sources predecessor'):
            _source_entries(self.objects,self.rows,replace(state,exits=(empty,)))

    def test_removed_actual_child_returns_cannot_leave_a_live_Source_handoff(self):
        from config_source_handoff import _source_entries
        import copy
        changed=replace(self.state.control,record=copy.deepcopy(self.state.control.record))
        changed.record['nested_returns']=()
        # The selected frontier/receiver still match. They cannot replace the
        # same current complete ordinary return/ingress relation.
        with self.assertRaisesRegex(AssertionError,'Sources predecessor.*control'):
            _source_entries(self.objects,self.rows,replace(self.state,control=changed))


if __name__=='__main__':unittest.main()
