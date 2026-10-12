"""Frame metadata and cumulative-budget regressions, including rejected fixes."""
import struct
import unittest
from check_stack_budget import frames, budgets, task_frames, configuration_frames


# Literal .stack_sizes/symbol evidence from slot2-demo280-probe2.
SLOT_CAPTURE = """
10034e74 36 _RNvNtNtCs6m2Zc6X1WBq_4core3str8converts9from_utf8
100416c4 768 _RNvMNtNtCsfF7AG8So2wt_16dashboard_config8validate4slotNtB2_10ConfigSlot12load_at_boot
1003add0 200 _RNvNtCsfF7AG8So2wt_16dashboard_config8validate12validate_ref
1003d090 24 _RNvMs0_NtCsfF7AG8So2wt_16dashboard_config5modelNtB5_9Placement8occupied
1003d12c 132 _RNvMs7_NtCsfF7AG8So2wt_16dashboard_config10appearanceNtB5_12WidgetColors12invalid_role
1003d3d8 8 _RNvMs0_NtCsfF7AG8So2wt_16dashboard_config10appearanceNtB5_5Gauge5valid
1003d454 24 _RNvMs8_NtCsfF7AG8So2wt_16dashboard_config10appearanceNtB5_5Color9contrasts
1003d7d4 56 _RNvNtCsfF7AG8So2wt_16dashboard_config7catalog10definition
1003db4c 8 _RNvXs1I_NtCsfF7AG8So2wt_16dashboard_config5modelNtB6_6AccessNtNtCs6m2Zc6X1WBq_4core3cmp9PartialEq2eq
1003d504 16 _RNvNtCsfF7AG8So2wt_16dashboard_config4fuel13stoichiometry
1003dbc0 40 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config5model4ViewEB2j_
1003dce0 296 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config5model6WidgetEB2j_
10040d88 16 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config5model10GraphRangeEB2j_
10040e88 144 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config5model15WarningSettingsEB2j_
100410e8 16 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config10appearance5GaugeEB2j_
100411f8 88 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config10appearance12WidgetColorsEB2j_
1003e1d8 424 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementANtNtCsfF7AG8So2wt_16dashboard_config5model13ChannelSourcej8_EB2k_
100406b8 72 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config5model13ChannelSourceEB2j_
1003e458 24 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config10appearance13WarningColorsEB2j_
1003e52c 56 _RINvNtCsfF7AG8So2wt_16dashboard_config5codec17compare_canonicalNtNtB4_5model6ConfigEB4_
1003e820 56 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldINtNtCs7NMQQ0HDubs_8heapless3vec8VecInnerNtNtB1a_5model6WidgetjINtNtB34_7storage15VecStorageInnerAINtNtNtCs6m2Zc6X1WBq_4core3mem12maybe_uninit11MaybeUninitB3G_Ej40_EEEB1a_
1003f378 40 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldANtNtB1a_5model13ChannelSourcej8_EB1a_
1003fa4c 32 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_5model19WarningPresentationEB1a_
1003fb24 32 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_5model4FuelEB1a_
1003fcb4 40 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_10appearance13WarningColorsEB1a_
1003fdac 40 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_8controls10ControlMapEB1a_
10040388 32 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_10appearance5ColorEB1a_
10040354 12 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldfEB1a_
100401b0 8 _RINvXs5_NtNtCshsQS6kZK4q9_8postcard3ser10serializerQINtB6_10SerializerNtNtCsfF7AG8So2wt_16dashboard_config5codec7CompareENtNtCscTQNy2XXAKm_10serde_core3ser15SerializeStruct15serialize_fieldNtNtB1a_8controls13ControlActionEB1a_
10040418 64 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config8controls14ScreenBindingsEB2j_
100405fc 16 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config8controls13ControlActionEB2j_
1004153c 16 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementNtNtCsfF7AG8So2wt_16dashboard_config10appearance5ColorEB2j_
100415bc 48 _RINvYINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCscTQNy2XXAKm_10serde_core2de9SeqAccess12next_elementINtNtCs7NMQQ0HDubs_8heapless6string11StringInnerjINtNtNtB2k_3vec7storage15VecStorageInnerAINtNtNtCs6m2Zc6X1WBq_4core3mem12maybe_uninit11MaybeUninithEj14_EEECsfF7AG8So2wt_16dashboard_config
10042288 28 _RINvNtCsfF7AG8So2wt_16dashboard_config15decode_in_place4nextNtNtB4_5model19WarningPresentationINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB1z_7flavors5SliceEEB4_
1004249c 36 _RINvNtCsfF7AG8So2wt_16dashboard_config15decode_in_place4nextNtNtB4_5model4FuelINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB1j_7flavors5SliceEEB4_
10042830 24 _RINvNtCsfF7AG8So2wt_16dashboard_config15decode_in_place4nextNtNtB4_10appearance13WarningColorsINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB1z_7flavors5SliceEEB4_
10042918 120 _RINvNtCsfF7AG8So2wt_16dashboard_config15decode_in_place4nextNtNtB4_8controls10ControlMapINtNtNtCshsQS6kZK4q9_8postcard2de12deserializer9SeqAccessNtNtB1t_7flavors5SliceEEB4_
"""


class StackBudget(unittest.TestCase):
    def slot_fixture(self):
        rows = [line.split(maxsplit=2) for line in SLOT_CAPTURE.splitlines() if line]
        return ({n: (int(a, 16) | 1, 4, 0) for a, _, n in rows},
                {int(a, 16): int(size) for a, size, _ in rows})

    def test_slot_capture_counts_nested_children_and_sibling_maximum(self):
        functions, sizes = self.slot_fixture()
        # 768 loader + 296 Widget + 144 warning + 48 text + 36 UTF-8.
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        # A larger alternate source-array child must compete with that branch.
        source = next(a for n, (a, _, _) in functions.items() if 'next_elementA' in n)
        sizes[source & ~1] = 1000
        self.assertEqual(configuration_frames(functions, sizes), 1856)

    def test_each_required_slot_child_needs_a_symbol_and_frame(self):
        functions, sizes = self.slot_fixture()
        for name, (address, _, _) in functions.items():
            with self.subTest(name=name):
                without_frame = sizes.copy()
                del without_frame[address & ~1]
                with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
                    configuration_frames(functions, without_frame)
                without_symbol = functions.copy()
                del without_symbol[name]
                with self.assertRaisesRegex(AssertionError, 'missing or ambiguous'):
                    configuration_frames(without_symbol, sizes)

    def test_emitted_seed_and_fallback_frames_are_counted(self):
        for symbol, frame, expected in (
            ('dashboard_config_ConfigSeed_visit_seq', 64, 1356),
            ('dashboard_config_VecSeed_6Widget_visit_seq', 64, 1356),
            ('dashboard_config_19fill_default_config', 2000, 2824),
        ):
            with self.subTest(symbol=symbol):
                functions, sizes = self.slot_fixture()
                functions[symbol] = (1, 4, 0)
                with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
                    configuration_frames(functions, sizes)
                sizes[0] = frame
                self.assertEqual(configuration_frames(functions, sizes), expected)

    def test_outlined_decode_wraps_canonical_and_validation_too(self):
        functions, sizes = self.slot_fixture()
        functions['dashboard_config_19decode_current_into'] = (1, 4, 0)
        sizes[0] = 64
        validator = next(a for n, (a, _, _) in functions.items() if '12validate_ref' in n)
        sizes[validator & ~1] = 2000
        self.assertEqual(configuration_frames(functions, sizes), 2964)

    def test_aliases_deduplicate_but_unknown_or_ambiguous_paths_fail(self):
        functions, sizes = self.slot_fixture()
        name = next(n for n in functions if '12load_at_boot' in n)
        functions[name + '.alias'] = functions[name]
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        functions[name + '.alias'] = (1, 4, 0)
        sizes[0] = 64
        with self.assertRaisesRegex(AssertionError, 'ambiguous'):
            configuration_frames(functions, sizes)
        del functions[name + '.alias']
        functions['dashboard_config_decode_in_place_unreviewed_child'] = (1, 4, 0)
        with self.assertRaisesRegex(AssertionError, 'unreviewed config stack helpers'):
            configuration_frames(functions, sizes)

    def test_outlined_future_body_cannot_hide_behind_small_poll_shim(self):
        functions = {'TaskStorage_dashboard_task_4poll': (101, 1, 0),
                     'dashboard_task_body': (201, 1, 0),
                     'dashboard_task_alias': (201, 1, 0),
                     'dashboard_task_write_in_place': (301, 1, 0)}
        self.assertEqual(task_frames(functions, {100: 16, 200: 17936}, 'dashboard_task'), 17952)
        with self.assertRaises(AssertionError):
            task_frames(functions, {100: 16}, 'dashboard_task')
        with self.assertRaises(AssertionError):
            task_frames({}, {}, 'dashboard_task')

    def test_metadata_decodes_thumb_addresses_and_variable_sizes(self):
        self.assertEqual(frames(struct.pack('<I', 0x10000101) + b'\xac\x02'), {0x10000100: 300})

    def test_metadata_rejects_truncation_overflow_and_conflicting_duplicates(self):
        for data in (b'', b'abc', struct.pack('<I', 1), struct.pack('<I', 1) + b'\x80',
                     struct.pack('<I', 1) + b'\xff\xff\xff\xff\x10',
                     struct.pack('<I', 1) + b'\x80\x00',
                     struct.pack('<IBIB', 1, 2, 1, 3)):
            with self.subTest(data=data), self.assertRaises(AssertionError):
                frames(data)

    def test_original_overflow_and_incomplete_helper_fix_are_rejected(self):
        for poll, constructor in ((38576, 0), (22528, 31808), (5400, 31520)):
            with self.subTest(poll=poll), self.assertRaises(AssertionError):
                budgets(50344, poll + constructor + 1024, 10056, 30000, 68648)

    def test_boot_nested_decode_regression_is_rejected(self):
        with self.assertRaises(AssertionError):
            budgets(87124, 25456, 10056, 30000, 68648)

    def test_final_candidate_and_exact_reserve_boundaries(self):
        budgets(50344, 25456, 10056, 30000, 68648)
        budgets(65536 - 8192, 32768 - 4096, 32768 - 4096, 65536 - 8192, 65536)
        for boot, construct, runtime, dashboard, headroom in (
            (65536 - 8192 + 1, 0, 0, 0, 65536),
            (0, 32768 - 4096 + 1, 0, 0, 65536),
            (0, 0, 32768 - 4096 + 1, 0, 65536),
            (0, 0, 0, 65536 - 8192 + 1, 65536),
        ):
            with self.assertRaises(AssertionError):
                budgets(boot, construct, runtime, dashboard, headroom)


if __name__ == '__main__':
    unittest.main()
