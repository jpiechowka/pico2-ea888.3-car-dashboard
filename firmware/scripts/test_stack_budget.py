"""Frame metadata and cumulative-budget regressions, including rejected fixes."""
import json
from pathlib import Path
import struct
import unittest
from check_stack_budget import frames, budgets, task_frames, configuration_frames, REVIEWED_STARTUP_FRAMES


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

    def orientation_fixture(self):
        # Literal linked symbols/.stack_sizes from ELF SHA in this fixture.
        record=json.loads((Path(__file__).parent/'fixtures/orientation-stack-demo280.json').read_text())
        return ({r['name']:(r['address']|1,4,0) for r in record['functions']},
                {r['address']:r['frame'] for r in record['functions']})

    def test_orientation_capture_inlined_widget_and_exact_children(self):
        functions,sizes=self.orientation_fixture()
        self.assertEqual(configuration_frames(functions,sizes),2288)
        self.assertFalse(any('12next_element' in n and '6WidgetE' in n for n in functions))
        # Parent-frame growth is always included, without relaxing reserves.
        loader=next(a&~1 for n,(a,_,_) in functions.items() if '12load_at_boot' in n)
        sizes[loader]+=4096
        self.assertEqual(configuration_frames(functions,sizes),6384)

    def test_orientation_serializer_folded_secondary_keeps_widget_parent(self):
        functions,sizes=self.orientation_fixture()
        child=next(a&~1 for n,(a,_,_) in functions.items()
                   if '15serialize_field' in n and '11orientation11Orientation' in n)
        # Frozen ELF 0x1004888a in Widget calls this shared enum serializer.
        # Growing the child must retain loader1280 + canonical56 + Widget72,
        # plus the160-byte conservative startup-helper sum.
        sizes[child]=4096
        self.assertEqual(configuration_frames(functions,sizes),5664)
        # If LLVM retains both names for that address, do not double-count it.
        functions['dashboard_config_15serialize_field_9Secondary']=(child|1,4,0)
        self.assertEqual(configuration_frames(functions,sizes),5664)

    def test_orientation_required_boundaries_fail_missing_ambiguous_or_metadata(self):
        functions,sizes=self.orientation_fixture()
        for tokens in [('12load_at_boot',),('15decode_in_place4next','11Orientation'),
                       ('15serialize_field','11Orientation'),('9Placement11occupied_in',),
                       ('12next_element','15AdvancedDisplay'),('12next_element','15WarningSettings')]:
            name=next(n for n in functions if all(t in n for t in tokens))
            address,length,section=functions[name]
            for fault in ('missing','metadata','ambiguous'):
                changed=dict(functions);frames=dict(sizes)
                if fault=='missing':del changed[name]
                elif fault=='metadata':del frames[address&~1]
                else:
                    changed[name+'.extra']=(0x30000001,length,section);frames[0x30000000]=4
                with self.subTest(tokens=tokens,fault=fault),self.assertRaises(AssertionError):
                    configuration_frames(changed,frames)
        for token in ('12next_element_6WidgetE','15serialize_field_9Secondary'):
            changed=dict(functions);frames=dict(sizes)
            changed['dashboard_config_'+token]=(0x30000001,4,0);frames[0x30000000]=4096
            self.assertGreater(configuration_frames(changed,frames),4096)
            changed['dashboard_config_'+token+'.other']=(0x30000005,4,0);frames[0x30000004]=4
            with self.assertRaisesRegex(AssertionError,'ambiguous'):
                configuration_frames(changed,frames)
        functions['dashboard_config_orientation_unreviewed_child']=(0x30000001,4,0)
        sizes[0x30000000]=8
        with self.assertRaisesRegex(AssertionError,'unreviewed config stack helpers'):
            configuration_frames(functions,sizes)

    def test_exact_startup_helpers_count_and_reject_drift(self):
        functions, sizes = self.slot_fixture()
        baseline = configuration_frames(functions, sizes)
        for index, (name, ceiling) in enumerate(REVIEWED_STARTUP_FRAMES.items()):
            name = name.replace('Cs_', 'CsReviewed_')
            address = 0x11000000 + index * 1024
            functions[name] = (address | 1, 42, 0)
            sizes[address] = ceiling
        self.assertEqual(configuration_frames(functions, sizes), baseline + 160)
        for name in list(functions):
            if 'CsReviewed_' not in name:
                continue
            address = functions[name][0] & ~1
            old = sizes[address]
            for invalid in [old + 4, -1, True, None]:
                sizes[address] = invalid
                with self.assertRaises(AssertionError):
                    configuration_frames(functions, sizes)
            del sizes[address]
            with self.assertRaises(AssertionError):
                configuration_frames(functions, sizes)
            sizes[address] = old
            entry = functions.pop(name)
            functions[name + 'unreviewed'] = entry
            with self.assertRaises(AssertionError):
                configuration_frames(functions, sizes)
            del functions[name + 'unreviewed']
            functions[name] = entry
            functions[name.replace('CsReviewed_', 'CsOther_')] = (address + 513, 42, 0)
            with self.assertRaises(AssertionError):
                configuration_frames(functions, sizes)
            del functions[name.replace('CsReviewed_', 'CsOther_')]

    def guarded_fixture(self):
        functions, sizes = self.slot_fixture()
        replacements = {
            '12WidgetColors12invalid_role': ('_RINvMs7_NtCsfF7AG8So2wt_16dashboard_config10appearanceNtB6_12WidgetColors19visit_invalid_rolesNCINvNtB8_8validate16visit_validationNCNvB1z_12validate_ref0Es8_0EB8_', 140),
            '4fuel13stoichiometry': ('_RINvNtCsfF7AG8So2wt_16dashboard_config4fuel19visit_stoichiometryNCINvNtB4_8validate16visit_validationNCNvB15_12validate_ref0Esc_0EB4_', 24),
        }
        for token, (new, frame) in replacements.items():
            old = next(n for n in functions if token in n)
            address, length, section = functions.pop(old)
            functions[new] = (address, length, section)
            sizes[address & ~1] = frame
        adapter = next(a & ~1 for n, (a, _, _) in functions.items() if '8validate12validate_ref' in n)
        sizes[adapter] = 288
        return functions, sizes

    def test_guarded_helpers_embedding_adapter_name_are_counted_as_siblings(self):
        functions, sizes = self.guarded_fixture()
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        color = next(a & ~1 for n, (a, _, _) in functions.items() if '19visit_invalid_roles' in n)
        fuel = next(a & ~1 for n, (a, _, _) in functions.items() if '19visit_stoichiometry' in n)
        sizes[color] = 2000
        self.assertEqual(configuration_frames(functions, sizes), 3056)
        sizes[fuel] = 2400
        # loader768 + adapter288 + max(colour2000, fuel2400), not their sum.
        self.assertEqual(configuration_frames(functions, sizes), 3456)

    def test_guarded_helpers_require_exact_symbols_metadata_and_review_new_outlining(self):
        functions, sizes = self.guarded_fixture()
        for token in ['8validate12validate_ref', '19visit_invalid_roles', '19visit_stoichiometry']:
            name = next(n for n in functions if token in n)
            without_symbol = dict(functions)
            del without_symbol[name]
            with self.assertRaisesRegex(AssertionError, 'missing or ambiguous'):
                configuration_frames(without_symbol, sizes)
            without_frame = dict(sizes)
            del without_frame[functions[name][0] & ~1]
            with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
                configuration_frames(functions, without_frame)
        # A newly outlined visitor is not silently exempted as a validation helper.
        functions['dashboard_config8validate16visit_validation'] = (1, 4, 0)
        sizes[0] = 2000
        with self.assertRaisesRegex(AssertionError, 'unreviewed config stack helpers'):
            configuration_frames(functions, sizes)

    def test_v4_visitors_led_history_and_serializer_children_are_counted(self):
        functions, sizes = self.slot_fixture()
        old = next(n for n in functions if '12next_elementA' in n)
        address, _, _ = functions.pop(old)
        functions['dashboard_config_15decode_in_place4nextA_13ChannelSource'] = (address, 4, 0)
        sizes[address & ~1] = 760
        additions = {
            '12next_element_15AdvancedDisplay': 384,
            '12next_element_13PeakSelection': 16,
            '12next_element_8Priority': 16,
            '12next_elementb': 8,
            '15decode_in_place4nextAtj8_': 56,
            '15decode_in_place4nextb': 8,
            '15decode_in_place4next_11LedSettings': 80,
            '12next_element_10LedPattern': 32,
            '15serialize_fieldAtj8_': 40,
            '15serialize_field_11LedSettings': 16,
            '15serialize_field_10LedPattern': 40,
            '15serialize_fieldb': 8,
            '15serialize_field_7Readout': 8,
            '15serialize_field_9Secondary': 8,
            '15serialize_field_13PeakSelection': 8,
        }
        for i, (name, size) in enumerate(additions.items()):
            address = 0x30000000 + 4 * i
            functions['dashboard_config_' + name] = (address | 1, 4, 0)
            sizes[address] = size
        # Existing loader768 + v4 source array760 + element72 + scalar16.
        self.assertEqual(configuration_frames(functions, sizes), 1616)
        child = next(a & ~1 for n, (a, _, _) in functions.items() if '15AdvancedDisplay' in n)
        sizes[child] = 2000
        self.assertEqual(configuration_frames(functions, sizes), 3100)
        # A newly emitted primitive remains a checked child, not an exemption.
        missing = dict(sizes)
        del missing[child]
        with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
            configuration_frames(functions, missing)

    def test_runtime_generic_snapshot_is_not_a_boot_config_child(self):
        functions, sizes = self.slot_fixture()
        # Exact new runtime symbol, whose generic config pointee appears in its name.
        owner = '_RNvMs6_NtCsd7SjC8HJ0mV_14dashboard_core11acquisitionINtB5_16AcquisitionOwnerRNtNtCsfF7AG8So2wt_16dashboard_config8validate15ValidatedConfigE8snapshotB7_'
        functions[owner] = (1, 4, 0)
        sizes[0] = 8
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        # The exemption is one runtime method, not the owner type or config module.
        functions[owner.replace('E8snapshot', 'E6new_op')] = (9, 4, 0)
        sizes[8] = 8
        with self.assertRaisesRegex(AssertionError, 'unreviewed config stack helpers'):
            configuration_frames(functions, sizes)

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

    def test_emitted_validation_children_require_metadata_and_count_with_parent(self):
        for symbol in (
            '_RNvNtCsfF7AG8So2wt_16dashboard_config8controls17validate_controls',
            '_RNvNtCsfF7AG8So2wt_16dashboard_config8validate10text_valid',
        ):
            for frame in (None, 9000):
                with self.subTest(symbol=symbol, frame=frame):
                    functions, sizes = self.slot_fixture()
                    functions[symbol] = (0x20000001, 4, 0)
                    if frame is None:
                        with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
                            configuration_frames(functions, sizes)
                    else:
                        sizes[0x20000000] = frame
                        # Loader 768 + validator 200 + its emitted child 9000.
                        self.assertEqual(configuration_frames(functions, sizes), 9968)

    def test_unreviewed_validation_descendants_cannot_hide_by_module_or_name(self):
        for suffix in ('8validate8in_range', '8controls13ControlAction10allowed_on',
                       '10appearance5Color9luminance', '7catalog14minimum_period',
                       '5model7Channel13freshness_cap', '4fuel10new_helper'):
            for frame in (None, 9000):
                with self.subTest(suffix=suffix, frame=frame):
                    functions, sizes = self.slot_fixture()
                    functions['_RNvNtCsfF7AG8So2wt_16dashboard_config' + suffix] = (1, 4, 0)
                    if frame is not None:
                        sizes[0] = frame
                    with self.assertRaisesRegex(AssertionError, 'unreviewed config stack helpers'):
                        configuration_frames(functions, sizes)

    def test_explicit_runtime_comparisons_and_bounded_panic_formatting(self):
        functions, sizes = self.slot_fixture()
        runtime = ('_RNvXsS_NtCsfF7AG8So2wt_16dashboard_config5modelNtB5_14DisplayOptions'
                   'NtNtCs6m2Zc6X1WBq_4core3cmp9PartialEq2eq')
        functions[runtime] = (1, 4, 0)
        # This comparator belongs to the dashboard runtime, not boot loading.
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        debug = ('_RNvXsh_NtCsfF7AG8So2wt_16dashboard_config8validateNtB5_15ValidationError'
                 'NtNtCs6m2Zc6X1WBq_4core3fmt5Debug3fmt')
        functions[debug] = (101, 4, 0)
        with self.assertRaisesRegex(AssertionError, 'missing config frame metadata'):
            configuration_frames(functions, sizes)
        sizes[100] = 80
        self.assertEqual(configuration_frames(functions, sizes), 1292)
        sizes[100] = 9000
        with self.assertRaisesRegex(AssertionError, 'config panic formatting frame'):
            configuration_frames(functions, sizes)

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


class PublicationFrames(unittest.TestCase):
    def test_outlined_publication_counts_nested_snapshot_helpers_and_requires_metadata(self):
        from check_stack_budget import publication_frames
        functions = {
            'pico2_tasks_acquisition_publish_snapshot': (0x101, 4, 0),
            'dashboard_core_AcquisitionModel_8snapshot': (0x201, 4, 0),
            'dashboard_core_SensorState_22snapshot_with_identity': (0x301, 4, 0),
        }
        # Literal cache5-vehicle280-traced nested frames.
        sizes = {0x100: 3792, 0x200: 4400, 0x300: 168}
        self.assertEqual(publication_frames(functions, sizes), 8360)
        for address in sizes:
            reduced = dict(sizes)
            del reduced[address]
            with self.assertRaisesRegex(AssertionError, 'missing publication frame metadata'):
                publication_frames(functions, reduced)

        for name in list(functions)[:2]:
            without = dict(functions)
            del without[name]
            with self.assertRaisesRegex(AssertionError, 'missing or ambiguous publication boundary'):
                publication_frames(without, sizes)


class OwnerPublicationFrames(unittest.TestCase):
    def test_fixed_variants_and_measured_source_origin_children_are_counted(self):
        from check_stack_budget import constructor_frames
        functions = {
            'dashboard_core_PreparedVehicle_16assemble_variant_const_true': (0x101, 4, 0),
            'dashboard_core_PreparedVehicle_16assemble_variant_const_false': (0x201, 4, 0),
            'dashboard_core_PreparedSession_16assemble_variant_const_true': (0x301, 4, 0),
            'dashboard_core_OriginIssuer_5issue': (0x401, 4, 0),
            'dashboard_config_7catalog_14resolve_source': (0x501, 4, 0),
            'dashboard_config_7catalog_10definition': (0x601, 4, 0),
        }
        sizes = {0x100: 20000, 0x200: 20000, 0x300: 200, 0x400: 8, 0x500: 72, 0x600: 52}
        # Both siblings remain in the conservative address sum.
        self.assertEqual(constructor_frames(functions, sizes), 40332)
        for address in sizes:
            reduced = dict(sizes)
            del reduced[address]
            with self.subTest(missing=address), self.assertRaisesRegex(AssertionError, 'constructor frame metadata'):
                constructor_frames(functions, reduced)
            increased = dict(sizes)
            increased[address] += 32
            self.assertEqual(constructor_frames(functions, increased), 40364)

    def test_borrowed_initialization_and_constant_base_frames_require_metadata(self):
        from check_stack_budget import constructor_frames
        functions = {
            'dashboard_core_AcquisitionOwner_session_base': (0x101, 4, 0),
            'dashboard_core_AcquisitionOwner_initialize_for_session': (0x201, 4, 0),
            'dashboard_core_AcquisitionOwner_configured_sources': (0x301, 4, 0),
            'dashboard_core_AcquisitionOwner_initialize_bound_policy': (0x401, 4, 0),
            'dashboard_core_RuleEngineState_initialize_bound_policy': (0x501, 4, 0),
            'dashboard_core_PreparedSession_construction_base': (0x601, 4, 0),
            'dashboard_core_Session_initialize_measurements': (0x701, 4, 0),
            'dashboard_core_AcquisitionState_4base': (0x801, 4, 0),
        }
        sizes = {address & ~1: 32 for address, _, _ in functions.values()}
        self.assertEqual(constructor_frames(functions, sizes), 256)
        for address in sizes:
            reduced = dict(sizes)
            del reduced[address]
            with self.subTest(missing=address), self.assertRaisesRegex(AssertionError, 'constructor frame metadata'):
                constructor_frames(functions, reduced)

    def test_demo_and_unavailable_constructor_boundary_is_counted_and_requires_metadata(self):
        from check_stack_budget import constructor_frames
        functions = {'pico2_tasks_acquisition_configured_model': (0x101, 4, 0)}
        self.assertEqual(constructor_frames(functions, {0x100: 24000}), 24000)
        with self.assertRaisesRegex(AssertionError, 'missing constructor frame metadata'):
            constructor_frames(functions, {})

    def test_prepared_session_assembly_is_counted_and_requires_metadata(self):
        from check_stack_budget import constructor_frames
        functions = {'dashboard_core_diagnostics_session_PreparedSession_8assemble': (0x101, 4, 0)}
        self.assertEqual(constructor_frames(functions, {0x100: 17000}), 17000)
        with self.assertRaisesRegex(AssertionError, 'missing constructor frame metadata'):
            constructor_frames(functions, {})

    def test_whole_vehicle_preparation_and_assembly_are_counted(self):
        from check_stack_budget import constructor_frames
        functions = {'dashboard_core_diagnostics_service_PreparedVehicle_8assemble': (0x101, 4, 0),
                     'dashboard_core_diagnostics_service_Vehicle_13prepare_inner': (0x201, 4, 0)}
        self.assertEqual(constructor_frames(functions, {0x100: 20000, 0x200: 400}), 20400)
        for address in [0x100, 0x200]:
            reduced = {0x100: 20000, 0x200: 400}
            del reduced[address]
            with self.assertRaisesRegex(AssertionError, 'missing constructor frame metadata'):
                constructor_frames(functions, reduced)

    def fixture(self):
        functions = {
            'pico2_tasks_acquisition_publish_snapshot': (0x101, 4, 0),
            'dashboard_core_AcquisitionOwner_8snapshot': (0x201, 4, 0),
            'dashboard_core_AcquisitionState_8snapshot': (0x301, 4, 0),
            'dashboard_core_SensorState_22snapshot_with_identity': (0x401, 4, 0),
            'dashboard_core_Session_8snapshot': (0x501, 4, 0),
        }
        return functions, {0x100: 100, 0x200: 8, 0x300: 4000, 0x400: 168, 0x500: 16}

    def test_owner_and_storage_nested_frames_include_forwarders_once(self):
        from check_stack_budget import publication_frames
        functions, sizes = self.fixture()
        functions['dashboard_core_Vehicle_8snapshot'] = functions['dashboard_core_Session_8snapshot']
        self.assertEqual(publication_frames(functions, sizes), 4292)

    def test_owner_storage_boundaries_and_every_frame_fail_closed(self):
        from check_stack_budget import publication_frames
        functions, sizes = self.fixture()
        for name in list(functions)[:3]:
            without = dict(functions)
            del without[name]
            with self.subTest(missing=name), self.assertRaisesRegex(AssertionError, 'publication boundary'):
                publication_frames(without, sizes)
            duplicate = dict(functions)
            duplicate[name + '_other_instance'] = (0x601, 4, 0)
            with self.subTest(ambiguous=name), self.assertRaisesRegex(AssertionError, 'publication boundary'):
                publication_frames(duplicate, sizes)
        for address in sizes:
            without = dict(sizes)
            del without[address]
            with self.subTest(metadata=address), self.assertRaisesRegex(AssertionError, 'publication frame metadata'):
                publication_frames(functions, without)

    def test_emitted_owner_storage_policy_constructors_all_require_metadata(self):
        from check_stack_budget import constructor_frames
        functions = {
            'pico2_configured_vehicle': (0x101, 4, 0),
            'dashboard_core_AcquisitionOwner_with_configuration': (0x201, 4, 0),
            'dashboard_core_AcquisitionState_with_policy': (0x301, 4, 0),
            'dashboard_core_RuleEngineState_3new': (0x401, 4, 0),
            'dashboard_core_Session_new_inner': (0x501, 4, 0),
        }
        sizes = {0x100: 100, 0x200: 80, 0x300: 200, 0x400: 40, 0x500: 10}
        self.assertEqual(constructor_frames(functions, sizes), 430)
        functions['dashboard_core_Vehicle_configured'] = functions['dashboard_core_Session_new_inner']
        self.assertEqual(constructor_frames(functions, sizes), 430)
        for address in sizes:
            without = dict(sizes)
            del without[address]
            with self.subTest(metadata=address), self.assertRaisesRegex(AssertionError, 'constructor frame metadata'):
                constructor_frames(functions, without)


if __name__ == '__main__':
    unittest.main()
