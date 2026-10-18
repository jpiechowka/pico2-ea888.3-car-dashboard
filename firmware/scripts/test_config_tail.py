"""Focused Tail checks using retained machine rows and explicit abstract inputs."""

import unittest
from dataclasses import replace


class TailLiteralBodies(unittest.TestCase):
    @staticmethod
    def fixture(extent=None, delta=0):
        import json
        import struct
        from pathlib import Path
        from test_config_sources import ActualDecodedSources
        from test_config_stack import StartupConfigurationCopy, SameBLoaderBoundary
        from llvm_stack import Instruction

        objects, rows, source, item = ActualDecodedSources.fixture(extent, delta)
        B = objects.read_only(0x1001C654 + 7124, 4)
        extra = json.loads(
            (Path(__file__).parent / "fixtures/config-decoded-tail.json").read_bytes()
        )
        for at, encoded in extra["segments"]:
            StartupConfigurationCopy.put(objects, at, bytes.fromhex(encoded))
        for f in extra["functions"]:
            for name, address, extent, index in f["aliases"]:
                objects.elf.functions[name] = (address, extent, index)
            objects.elf.symbol_entries = list(
                dict.fromkeys(objects.elf.symbol_entries + [tuple(v) for v in f["symbols"]])
            )
            objects.elf.mapping_symbols.extend(tuple(v) for v in f["mapping"])
            if f["frame"] is not None:
                objects.sizes[f["address"]] = f["frame"]
        objects.elf.mapping_symbols = list(dict.fromkeys(objects.elf.mapping_symbols))
        for r in extra["records"]:
            rows[r["address"]] = Instruction(
                r["address"], bytes.fromhex(r["bytes"]), r["mnemonic"], r["operands"]
            )
        for offset in (3508, 7124):
            StartupConfigurationCopy.put(objects, 0x1001C654 + offset, B)
        return objects, rows, SameBLoaderBoundary.rebound(objects, source), item

    def test_complete_control_and_LED_bodies_bind_actual_targets_and_partial_output_order(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        objects, rows, _, _ = self.fixture()
        expected = {
            "ControlAction": (190, 16),
            "ButtonBinding": (90, 24),
            "ScreenBindings": (312, 64),
            "LedPattern": (200, 48),
        }
        for shape, want in expected.items():
            proof = tail_coverage(objects, _name(objects, ROLES[shape]), rows)
            self.assertEqual((proof["extent"], proof["frame"]), want)
            self.assertNotIn("completed_value", proof)
        action = tail_coverage(objects, _name(objects, ROLES["ControlAction"]), rows)
        self.assertEqual(
            action["action_targets"],
            tuple(
                0x1001BC64 + i
                for i in (
                    72,
                    138,
                    114,
                    126,
                    96,
                    150,
                    162,
                    132,
                    180,
                    108,
                    174,
                    90,
                    102,
                    156,
                    84,
                    120,
                    78,
                    144,
                    168,
                )
            ),
        )
        self.assertNotIn(0x1001BC64 + 71, action["action_targets"])
        screen = tail_coverage(objects, _name(objects, ROLES["ScreenBindings"]), rows)
        self.assertTrue(
            any(
                e[0] == 250 and e[1] == "b.w" and e[3] == ROLES["OUTLINED_FUNCTION_75"]
                for e in screen["ordinary_edges"]
            )
        )

    def test_coherent_table_guard_output_and_saved_tail_mutants_reject(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import StartupConfigurationCopy
        from llvm_stack import Instruction

        for mutant in ("table", "guard", "output", "tail"):
            objects, rows, _, _ = self.fixture()
            shape = "ControlAction" if mutant in ("table", "guard") else "ScreenBindings"
            if mutant == "table":
                StartupConfigurationCopy.put(objects, 0x1001BC64 + 52, b"\x0b")
            else:
                at = {
                    "guard": 0x1001BC64 + 44,
                    "output": 0x1001BB2C + 296,
                    "tail": 0x1001BB2C + 250,
                }[mutant]
                row = rows[at]
                if mutant == "guard":
                    raw = bytes((row.raw[0] ^ 1,)) + row.raw[1:]
                    operands = "r0, #0x13"
                    op = row.mnemonic
                elif mutant == "output":
                    raw = row.raw[:2] + b"\x07\x00"
                    operands = "r0, [r4, #0x7]"
                    op = row.mnemonic
                else:
                    raw = StartupConfigurationCopy.branch_link(at, 0x10028682)
                    operands = hex(0x10028682)
                    op = "bl"
                StartupConfigurationCopy.put(objects, at, raw)
                rows[at] = Instruction(at, raw, op, operands)
            with self.subTest(mutant=mutant), self.assertRaises(AssertionError):
                tail_coverage(objects, _name(objects, ROLES[shape]), rows)

    def test_Action_table_and_Screen_restore_relocate_by_current_typed_relationships(self):
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import ActualDecodedViews

        objects, rows, _, _ = self.fixture()
        ActualDecodedViews.move_function(objects, rows, ROLES["ControlAction"], 0x10071000)
        proof = tail_coverage(objects, _name(objects, ROLES["ControlAction"]), rows)
        self.assertEqual(proof["action_targets"][18], 0x10071000 + 168)
        ActualDecodedViews.move_function(objects, rows, ROLES["ScreenBindings"], 0x10072000)
        ActualDecodedViews.move_function(objects, rows, "OUTLINED_FUNCTION_75", 0x10073000)
        screen = tail_coverage(objects, _name(objects, ROLES["ScreenBindings"]), rows)
        self.assertTrue(
            any(
                e[2] == 0x10073000 and e[3] == ROLES["OUTLINED_FUNCTION_75"]
                for e in screen["ordinary_edges"]
            )
        )
        self.assertTrue(
            any(
                e[2] == 0x10071000 and e[3] == ROLES["ControlAction"]
                for e in screen["ordinary_edges"]
            )
        )


class TailStructuralInputs(unittest.TestCase):
    def test_repeated_current_coverage_reuses_structure_but_copies_returned_proofs(self):
        import copy
        from unittest.mock import patch
        import config_tail_children as tail
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_structural import _detached_inputs
        from config_decoded_sources import _phase_graph

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        name = _name(objects, ROLES["ControlAction"])
        with patch.object(tail, "audit_coverage", wraps=tail.audit_coverage) as audit:
            first = tail.tail_coverage(objects, name, rows)
            again = tail.tail_coverage(objects, name, rows, outline_role=None)
            self.assertEqual(first, again)
            self.assertEqual(_phase_graph(first), _phase_graph(again))
            self.assertIsNot(first, again)
            self.assertEqual(
                audit.call_count,
                1,
                "same detached Tail input must not repeat full-map ingress work",
            )
            saved = copy.deepcopy(again)
            first["objects"]["injected"] = "bad"
            self.assertEqual(tail.tail_coverage(objects=objects, name=name, rows=rows), saved)

    def test_exact_nine_APIs_and_context_keys_cannot_borrow_another_package(self):
        from config_structural import _APIS, _detached_inputs
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        original = {
            ("config_helpers", "helper_body"),
            ("config_helpers", "helper_coverage"),
            ("config_logger", "logger_body"),
            ("config_refill_children", "child_body"),
            ("config_refill_children", "child_coverage"),
            ("config_utf8", "utf8_body"),
            ("config_widget_children", "widget_body"),
        }
        sources = original | {("config_source_children", "source_coverage")}
        self.assertEqual(_APIS, sources | {("config_tail_children", "tail_coverage")})
        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        name = _name(objects, ROLES["ControlAction"])
        tail_coverage(objects, name, rows)
        for args in (
            (objects, name, dict(rows)),
            (objects, "unknown", rows),
            (objects, name, rows, "@outline:unknown"),
            (objects, name, rows, ROLES["OUTLINED_FUNCTION_75"]),
        ):
            with self.subTest(context=args[1:2]), self.assertRaises(AssertionError):
                tail_coverage(*args)

    def test_old_snapshot_detaches_and_new_consumed_table_and_frame_reject(self):
        from config_structural import _detached_inputs
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from test_config_stack import StartupConfigurationCopy

        for mutation in ("table", "frame"):
            objects, rows, _, _ = TailLiteralBodies.fixture()
            name = _name(objects, ROLES["ControlAction"])
            frozen, current = _detached_inputs(objects, rows)
            proof = tail_coverage(frozen, name, current)
            if mutation == "table":
                StartupConfigurationCopy.put(objects, 0x1001BC64 + 52, b"\x0b")
            else:
                objects.sizes[0x1001BC64] = 24
            self.assertEqual(tail_coverage(frozen, name, current), proof)
            fresh, new_rows = _detached_inputs(objects, rows)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                tail_coverage(fresh, name, new_rows)

    def test_unsupported_reader_rechecks_consumed_non_table_body_bytes(self):
        from config_stack import DiagnosticObjects
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_structural import _detached_inputs

        class Custom(DiagnosticObjects):
            reads = 0
            changed = False

            def read_only(self, address, extent):
                self.reads += 1
                raw = super().read_only(address, extent)
                if self.changed and address <= 0x1001BC64 + 44 < address + extent:
                    at = 0x1001BC64 + 44 - address
                    raw = raw[:at] + bytes((raw[at] ^ 1,)) + raw[at + 1 :]
                return raw

        objects, rows, _, _ = TailLiteralBodies.fixture()
        custom = Custom(objects.elf, objects.sizes)
        self.assertIs(_detached_inputs(custom, rows)[0], custom)
        name = _name(custom, ROLES["ControlAction"])
        first = tail_coverage(custom, name, rows)
        reads = custom.reads
        self.assertEqual(tail_coverage(custom, name, rows), first)
        self.assertGreater(custom.reads, reads)
        custom.changed = True
        with self.assertRaises(AssertionError):
            tail_coverage(custom, name, rows)


class TailChildReturns(unittest.TestCase):
    @staticmethod
    def evaluate(shape, mutation=None):
        from config_tail_children import tail_coverage, tail_children, tail_return_profiles
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_finite_returns import _closed_child_returns
        from config_structural import _detached_inputs
        from config_owner_entry import _direct_target

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        proof = tail_coverage(objects, _name(objects, ROLES[shape]), rows)
        off = {"ScreenBindings": 5712, "LedPattern": 6792}[shape]
        site = 0x1001C654 + off
        assert rows[site].mnemonic == "bl" and _direct_target(rows[site], "bl") == proof["function"]
        parent = dict(
            function=0x1001C654,
            ordinary_edges=((off, "bl", proof["function"], ROLES[shape], proof["frame"]),),
        )
        children = {
            (proof["function"], proof["role"]): proof,
            **tail_children(objects, rows, (proof,)),
        }
        if mutation:
            mutation(children)
        return _closed_child_returns(
            rows,
            (parent,),
            children,
            0x20070000,
            tail_return_profiles(),
            (ROLES["ControlAction"], ROLES["ButtonBinding"], ROLES["LedPattern"]),
            objects=objects,
        )

    def test_all_Action_arms_nested_Buttons_Screen_return_original_PC_and_callee_values(self):
        from config_tail_contracts import ROLES

        nodes = self.evaluate("ScreenBindings")
        screen = next(n for n in nodes if n["role"] == ROLES["ScreenBindings"])
        self.assertEqual(
            (screen["return_PC"], screen["entry_SP"], screen["return_SP"], screen["lowest_SP"]),
            (0x1001C654 + 5716, 0x20070000, 0x20070000, 0x20070000 - 152),
        )
        self.assertEqual(screen["preserved_high_gprs"], tuple("r" + str(i) for i in range(4, 12)))
        actions = [n for n in nodes if n["role"] == ROLES["ControlAction"]]
        self.assertTrue(actions)
        self.assertTrue(all(len(n["returns"]) == 21 for n in actions))
        self.assertTrue(
            all(
                n["preserved_high_gprs"] == tuple("r" + str(i) for i in range(4, 12))
                for n in actions
            )
        )
        self.assertFalse(screen["architectural_LR_preserved"])

    def test_LED_scratch_pop_restores_actual_original_high_saves_and_PC(self):
        from config_tail_contracts import ROLES

        nodes = self.evaluate("LedPattern")
        led = next(n for n in nodes if n["role"] == ROLES["LedPattern"])
        self.assertEqual(
            (led["return_PC"], led["return_SP"], led["lowest_SP"]),
            (0x1001C654 + 6796, 0x20070000, 0x20070000 - 96),
        )
        self.assertEqual(led["preserved_high_gprs"], tuple("r" + str(i) for i in range(4, 12)))
        self.assertFalse(led["architectural_LR_preserved"])

    def test_private_changed_Action_target_vector_cannot_authorize_return_walk(self):
        from config_tail_contracts import ROLES

        def changed(children):
            key = (0x1001BC64, ROLES["ControlAction"])
            children[key] = dict(children[key], action_targets=children[key]["action_targets"][:-1])

        with self.assertRaisesRegex(AssertionError, "guarded.*(Action|table)"):
            self.evaluate("ScreenBindings", changed)


class TailActionInterface(unittest.TestCase):
    @staticmethod
    def fixture(count=2, extent=32):
        from config_abstract import DecoderInterface, SequenceInterface, Span
        from config_regions import Storage, borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        objects, rows, _, _ = TailLiteralBodies.fixture()
        phase = "supplied_Tail_Action"
        visitor = Storage("stack:Tail", "supplied Visitor", 0x20070000, 1608, 8, phase, True)
        loader = Storage(
            visitor.space, "supplied loader", visitor.start + 1608, 456, 8, phase, True
        )
        screen = Storage(visitor.space, "supplied Screen", visitor.start - 64, 64, 8, phase, True)
        button = Storage(visitor.space, "supplied Button", screen.start - 24, 24, 8, phase, True)
        action = Storage(visitor.space, "supplied Action", button.start - 16, 16, 8, phase, True)
        payload = Storage("address", "supplied readonly input", 0x10032B2A, extent, 1, phase, False)
        decoder = DecoderInterface(
            borrow(loader, 236, 8, phase, True), borrow(payload, 0, extent, phase), Span(0, 0)
        )
        state = SequenceInterface(borrow(button, 0, 8, phase, True), decoder, Span(count, count))
        saved = Storage(
            visitor.space, "supplied Button original saves", button.start + 8, 16, 4, phase, False
        )
        proof = tail_coverage(objects, _name(objects, ROLES["ControlAction"]), rows)
        return objects, rows, proof, state, action, (saved,), 0x1001BD22 + 22

    def test_Action_source_domain_missing_and_error_origins_are_distinct(self):
        from config_tail_effects import control_action_interface

        result = control_action_interface(*self.fixture())
        self.assertEqual(result["normal"]["domain"], (0, 18))
        self.assertEqual(result["normal"]["after"].count.lower, 1)
        self.assertEqual(result["normal"]["after"].decoder.cursor.lower, 1)
        self.assertEqual(
            result["normal"]["mapping"],
            tuple(
                (i, t)
                for i, t in enumerate(
                    (
                        72,
                        138,
                        114,
                        126,
                        96,
                        150,
                        162,
                        132,
                        180,
                        108,
                        174,
                        90,
                        102,
                        156,
                        84,
                        120,
                        78,
                        144,
                        168,
                    )
                )
            ),
        )
        invalid = next(e for e in result["errors"] if e["stage"] == "Action domain")
        self.assertEqual(
            (invalid["error"], invalid["status"], invalid["child_outcome"]["kind"]),
            (14, 1, "success"),
        )
        self.assertIs(invalid["origin"]["before"], result["before"].decoder)
        none = control_action_interface(*self.fixture(count=0))
        self.assertIsNone(none["normal"])
        self.assertEqual((none["none"]["status"], none["none"]["action_low"]), (0, 255))
        self.assertEqual(none["primitive_invocations"], ())
        self.assertEqual(none["none"]["after"].count.lower, 0)

    def test_warmed_structure_does_not_cache_EOF_source_phase_or_protection(self):
        from config_tail_effects import control_action_interface
        from config_structural import _detached_inputs
        from config_abstract import Span
        from config_regions import Storage

        args = list(self.fixture())
        args[0], args[1] = _detached_inputs(args[0], args[1])
        result = control_action_interface(*args)
        self.assertIsNotNone(result["normal"])
        old = args[3]
        args[3] = replace(old, decoder=replace(old.decoder, cursor=Span(32, 32)))
        eof = control_action_interface(*args)
        self.assertIsNone(eof["normal"])
        self.assertEqual(tuple(e["error"] for e in eof["errors"]), (4,))
        args[3] = old
        frame = args[4]
        args[5] = args[5] + (
            Storage(
                "address",
                "physical deepest child overlap",
                frame.start - 48,
                1,
                1,
                frame.phase,
                False,
            ),
        )
        with self.assertRaisesRegex(AssertionError, "overlap|protected"):
            control_action_interface(*args)

    def test_wrong_current_caller_receiver_and_body_proof_reject(self):
        from config_tail_effects import control_action_interface
        from config_regions import borrow

        args = list(self.fixture())
        before = args[3]
        args[3] = replace(
            before, header=borrow(before.header.storage, 4, 8, before.header.storage.phase, True)
        )
        with self.assertRaises(AssertionError):
            control_action_interface(*args)
        args = list(self.fixture())
        args[-1] += 2
        with self.assertRaises(AssertionError):
            control_action_interface(*args)
        args = list(self.fixture())
        args[2] = dict(args[2], action_targets=())
        with self.assertRaises(AssertionError):
            control_action_interface(*args)


class TailButtonInterface(unittest.TestCase):
    @staticmethod
    def fixture(count=4, extent=32):
        from config_abstract import SequenceInterface, Span
        from config_regions import Storage, borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        objects, rows, _, action, _, _, _ = TailActionInterface.fixture(extent=extent)
        button = action.header.storage
        screen = Storage(
            button.space, "supplied Screen", button.start + 24, 64, 8, button.phase, True
        )
        sequence = SequenceInterface(
            borrow(screen, 20, 8, screen.phase, True), action.decoder, Span(count, count)
        )
        saved = Storage(
            screen.space, "Screen original saves", screen.start + 28, 36, 4, screen.phase, False
        )
        proof = tail_coverage(objects, _name(objects, ROLES["ButtonBinding"]), rows)
        return objects, rows, proof, sequence, button, (saved,), 0x1001BB2C + 30

    def test_tap_then_hold_keep_distinct_same_source_origins_and_packed_domains(self):
        from config_tail_effects import button_binding_interface

        result = button_binding_interface(*self.fixture())
        normal = result["normal"]
        self.assertIsNotNone(normal)
        self.assertEqual(normal["fields"], (("tap", (0, 18)), ("hold", (0, 18))))
        self.assertEqual(
            (normal["after"].count.lower, normal["after"].decoder.cursor.lower), (3, 2)
        )
        self.assertEqual(
            tuple(v["site"] for v in result["invocations"]), (0x1001BD22 + 22, 0x1001BD22 + 48)
        )
        first, second = result["invocations"]
        self.assertIs(second["before"], first["result"]["normal"]["after"])
        self.assertIs(second["predecessor"], first)
        self.assertEqual(normal["upper_bits"], "opaque; only low16 carry tap/hold")
        self.assertTrue(
            any(
                e["stage"] == "hold" and e["cause"]["origin"]["caller"] == 0x1001BD22 + 48
                for e in result["errors"]
            )
        )

    def test_short_hold_and_zero_outer_count_never_make_packed_success(self):
        from config_tail_effects import button_binding_interface

        short = button_binding_interface(*self.fixture(extent=1))
        self.assertIsNone(short["normal"])
        self.assertTrue(any(e["stage"] == "hold" and e["error"] == 4 for e in short["errors"]))
        missing = button_binding_interface(*self.fixture(count=0))
        self.assertIsNone(missing["normal"])
        self.assertEqual(missing["none"]["low"], 255)
        self.assertEqual(missing["invocations"], ())

    def test_wrong_screen_Q_or_ancestor_alias_rejects_with_structure_warm(self):
        from config_tail_effects import button_binding_interface
        from config_structural import _detached_inputs
        from config_regions import Storage, borrow

        args = list(self.fixture())
        args[0], args[1] = _detached_inputs(args[0], args[1])
        self.assertIsNotNone(button_binding_interface(*args)["normal"])
        before = args[3]
        args[3] = replace(
            before, header=borrow(before.header.storage, 12, 8, before.header.storage.phase, True)
        )
        with self.assertRaises(AssertionError):
            button_binding_interface(*args)
        args[3] = before
        frame = args[4]
        args[5] = args[5] + (
            Storage("address", "deeper Action alias", frame.start - 64, 1, 1, frame.phase, False),
        )
        with self.assertRaises(AssertionError):
            button_binding_interface(*args)


class TailOutputAggregates(unittest.TestCase):
    @staticmethod
    def fixture(shape="ScreenBindings", count=None, extent=64):
        from config_abstract import DecoderInterface, SequenceInterface, Span
        from config_regions import Storage, borrow
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        objects, rows, _, _ = TailLiteralBodies.fixture()
        phase = "supplied_Tail_output"
        visitor = Storage("stack:Tail-output", "supplied Visitor", 0x20070000, 1608, 8, phase, True)
        loader = Storage(
            visitor.space, "supplied loader", visitor.start + 1608, 456, 8, phase, True
        )
        payload = Storage("address", "supplied readonly input", 0x10032B2A, extent, 1, phase, False)
        decoder = DecoderInterface(
            borrow(loader, 236, 8, phase, True), borrow(payload, 0, extent, phase), Span(0, 0)
        )
        q, out, frame, default, site = (
            (1128, 940, 64, 3, 5712) if shape == "ScreenBindings" else (1000, 1064, 48, 7, 6792)
        )
        state = SequenceInterface(
            borrow(visitor, q, 8, phase, True),
            decoder,
            Span(default if count is None else count, default if count is None else count),
        )
        activation = Storage(
            visitor.space, "supplied " + shape, visitor.start - frame, frame, 8, phase, True
        )
        saved = Storage(
            visitor.space, "Visitor original saves", visitor.start + 1512, 96, 4, phase, False
        )
        proof = tail_coverage(objects, _name(objects, ROLES[shape]), rows)
        return (
            objects,
            rows,
            proof,
            state,
            borrow(visitor, out, 10, phase, True),
            activation,
            (saved,),
            0x1001C654 + site,
        )

    def test_Screen_source_order_domains_late_words_and_original_return(self):
        from config_tail_effects import screen_bindings_interface

        result = screen_bindings_interface(*self.fixture())
        normal = result["normal"]
        self.assertIsNotNone(normal)
        self.assertEqual(
            normal["source_order"],
            ("buttonA", "buttonB", "buttonX", "buttonY", "rotation", "click"),
        )
        self.assertEqual(normal["rotation_domain"], (0, 3))
        self.assertEqual(normal["click_domain"], (0, 18))
        self.assertEqual(normal["after"].decoder.cursor.lower, 10)
        self.assertEqual(normal["after"].count.lower, 2)
        self.assertEqual(
            normal["physical_output_order"], ((266, 1, 1), (268, 0, 1), (296, 6, 4), (306, 2, 4))
        )
        self.assertEqual(normal["outstanding_after_tag"], ((2, 8),))
        self.assertEqual(normal["initialized_bytes"], tuple(range(10)))
        self.assertTrue(
            any(
                e["stage"] == "rotation domain" and e["child_outcome"]["kind"] == "success"
                for e in result["errors"]
            )
        )
        self.assertTrue(result["requires_bound_Screen_word_access_policy"])

    def test_LED_Off_still_decodes_all_fields_and_keeps_opaque_padding(self):
        from config_tail_effects import led_pattern_interface

        result = led_pattern_interface(*self.fixture("LedPattern"))
        normal = result["normal"]
        self.assertEqual(
            normal["fields"],
            (
                ("kind", (0, 3)),
                ("color", (0, 65535)),
                ("brightness", (0, 255)),
                ("period_ms", (0, 65535)),
                ("on_percent", (0, 255)),
            ),
        )
        self.assertEqual(normal["after"].decoder.cursor.lower, 5)
        self.assertEqual(normal["after"].count.lower, 6)
        self.assertEqual(normal["initialized_bytes"], (0, 2, 3, 4, 6, 7, 8))
        self.assertEqual(normal["opaque_bytes"], (1, 5, 9))
        self.assertEqual(normal["physical_output_order"], ((184, 0, 1), (186, 6, 4), (194, 2, 4)))
        self.assertEqual(len(result["invocations"]), 5)
        self.assertTrue(result["requires_bound_LED_word_access_policy"])

    def test_short_missing_or_wrong_epoch_outputs_never_become_complete(self):
        from config_tail_effects import screen_bindings_interface, led_pattern_interface
        from config_regions import borrow

        for shape, interface, short in (
            ("ScreenBindings", screen_bindings_interface, 9),
            ("LedPattern", led_pattern_interface, 4),
        ):
            with self.subTest(shape=shape):
                result = interface(*self.fixture(shape, extent=short))
                self.assertIsNone(result["normal"])
                self.assertTrue(result["errors"])
                missing = interface(*self.fixture(shape, count=0))
                self.assertIsNone(missing["normal"])
                self.assertEqual(missing["none"]["tag"], 255)
                self.assertEqual(missing["invocations"], ())
                args = list(self.fixture(shape))
                output = args[4]
                args[4] = borrow(output.storage, output.offset + 4, 10, output.storage.phase, True)
                with self.assertRaises(AssertionError):
                    interface(*args)

    def test_Screen_receiver_output_and_deep_physical_guard_recheck_after_warmup(self):
        from config_tail_effects import screen_bindings_interface
        from config_structural import _detached_inputs
        from config_regions import Storage

        args = list(self.fixture())
        args[0], args[1] = _detached_inputs(args[0], args[1])
        self.assertIsNotNone(screen_bindings_interface(*args)["normal"])
        frame = args[5]
        guard = Storage(
            "address", "deepest descendant boundary", frame.start - 88, 1, 1, frame.phase, False
        )
        args[6] = args[6] + (guard,)
        with self.assertRaises(AssertionError):
            screen_bindings_interface(*args)

    def test_outer_None_still_protects_pushed_Screen_LED_frames_but_has_no_nested_child(self):
        from config_tail_effects import screen_bindings_interface, led_pattern_interface
        from config_regions import Storage

        for shape, interface in (
            ("ScreenBindings", screen_bindings_interface),
            ("LedPattern", led_pattern_interface),
        ):
            with self.subTest(shape=shape):
                args = list(self.fixture(shape, count=0))
                frame = args[5]
                original = args[6]
                args[6] = original + (
                    Storage(
                        "address",
                        "actual pushed None save",
                        frame.start + frame.extent - 1,
                        1,
                        1,
                        frame.phase,
                        False,
                    ),
                )
                with self.assertRaises(AssertionError):
                    interface(*args)
                args[6] = original + (
                    Storage(
                        "address",
                        "below inactive None child",
                        frame.start - 1,
                        1,
                        1,
                        frame.phase,
                        False,
                    ),
                )
                result = interface(*args)
                self.assertEqual(result["none"].get("physical_child_bytes"), frame.extent)
                self.assertEqual(
                    result["none"]["ordinary_return"]["return_SP"], frame.start + frame.extent
                )

    def test_LED_unwritten_padding_guard_survives_containing_output_envelope(self):
        from config_tail_effects import led_pattern_interface
        from config_regions import Storage

        args = list(self.fixture("LedPattern"))
        output = args[4]
        frame = args[5]
        original = args[6]
        pad = Storage(
            frame.space, "protected old LED padding", output.start + 1, 1, 1, frame.phase, False
        )
        args[6] = original + (pad,)
        result = led_pattern_interface(*args)
        self.assertIsNotNone(result["normal"])
        self.assertIn(pad, result["protected"])
        for offset in (0, 2, 5, 9):
            args[6] = original + (replace(pad, start=output.start + offset),)
            with self.subTest(offset=offset), self.assertRaises(AssertionError):
                led_pattern_interface(*args)


class TailParentBodies(unittest.TestCase):
    def test_all_parent_windows_bind_complete_tail_calls_same_B_and_restore(self):
        from config_decoded_tail import _parents

        objects, rows, _, _ = TailLiteralBodies.fixture()
        V = 0x1001C654
        B = 0x2000031C
        parents = _parents(objects, rows, V, B)
        self.assertEqual(
            tuple(p["function"] - V for p in parents), (170, 5630, 5922, 6018, 6412, 6450, 7000, 56)
        )
        edges = tuple(
            (p["function"] + e[0] - V, e[3]) for p in parents for e in p["ordinary_edges"]
        )
        from config_tail_contracts import ROLES

        self.assertEqual(
            tuple(at for at, role in edges if role == ROLES["ScreenBindings"]), (5712, 6198, 6426)
        )
        self.assertEqual(
            tuple(at for at, role in edges if role == ROLES["LedPattern"]), (6792, 6828)
        )
        self.assertIn((68, ROLES["OUTLINED_FUNCTION_155"]), edges)
        self.assertFalse(
            any(p["function"] <= V + 7122 < p["function"] + p["extent"] for p in parents)
        )
        with self.assertRaisesRegex(AssertionError, "same.*Config"):
            _parents(objects, rows, V, B + 4)

    def test_coherent_early_success_late_store_and_original_restore_changes_reject(self):
        from config_decoded_tail import _parents
        from test_config_stack import StartupConfigurationCopy
        from llvm_stack import Instruction

        for off in (7100, 7108, 68):
            objects, rows, _, _ = TailLiteralBodies.fixture()
            V = 0x1001C654
            at = V + off
            row = rows[at]
            if off == 68:
                raw = StartupConfigurationCopy.branch_link(at, 0x100289F0)
                op = "bl"
                args = hex(0x100289F0)
            else:
                raw = bytes.fromhex("00bf") * (len(row.raw) // 2)
                op = "nop" if len(raw) == 2 else "nop.w"
                args = ""
            StartupConfigurationCopy.put(objects, at, raw)
            rows[at] = Instruction(at, raw, op, args)
            with self.subTest(off=off), self.assertRaises(AssertionError):
                _parents(objects, rows, V, 0x2000031C)


class TailParentStructuralInputs(unittest.TestCase):
    def test_same_detached_parent_windows_do_not_repeat_literal_work_or_cache_B(self):
        import sys
        from config_decoded_tail import _parents
        from config_structural import _detached_inputs
        from config_decoded_sources import _phase_graph
        from config_finite import _literal_body

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        previous = sys.getprofile()
        calls = []

        def observe(frame, event, value):
            if (
                event == "call"
                and frame.f_code is _literal_body.__code__
                and str(frame.f_locals.get("shape", "")).startswith("decoded_tail_")
            ):
                calls.append(frame.f_locals["shape"])

        try:
            sys.setprofile(observe)
            first = _parents(objects, rows, 0x1001C654, 0x2000031C)
            second = _parents(objects, rows, 0x1001C654, 0x2000031C)
        finally:
            sys.setprofile(previous)
        self.assertEqual(first, second)
        self.assertEqual(_phase_graph(first), _phase_graph(second))
        self.assertEqual(
            len(calls), 8, "one detached parent bundle must match its eight windows once"
        )
        first[0]["objects"]["changed"] = True
        self.assertEqual(_parents(objects, rows, 0x1001C654, 0x2000031C), second)
        with self.assertRaisesRegex(AssertionError, "same.*Config"):
            _parents(objects, rows, 0x1001C654, 0x20000320)

    def test_parent_bundle_copies_stay_tagged_and_wrong_row_or_role_cannot_borrow_them(self):
        from config_tail_children import tail_coverage
        from config_headers import VISITOR
        from config_widget_children import _name
        from config_structural import _detached_inputs

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        name = _name(objects, VISITOR)
        result = tail_coverage(objects, name, rows)
        self.assertEqual(result["kind"], "tail_parent_literal_windows_only")
        self.assertFalse(
            set(result)
            & {
                "coverage",
                "owner",
                "B",
                "state",
                "source_context",
                "effects",
                "returns",
                "completed",
            }
        )
        result["windows"][0]["objects"]["injected"] = False
        fresh = tail_coverage(objects, name, rows, outline_role=None)
        self.assertNotIn("injected", fresh["windows"][0]["objects"])
        for other in (
            (objects, name, dict(rows)),
            (objects, name, rows, "@outline:tail_restore_high_low36"),
            (objects, "unknown", rows),
        ):
            with self.assertRaises(AssertionError):
                tail_coverage(*other)

    def test_old_parent_snapshot_detaches_and_fresh_body_pool_and_frame_reject(self):
        from config_decoded_tail import _parents
        from config_structural import _detached_inputs
        from test_config_stack import StartupConfigurationCopy

        for mutation in ("body", "pool", "frame"):
            objects, rows, _, _ = TailLiteralBodies.fixture()
            frozen, current = _detached_inputs(objects, rows)
            saved = _parents(frozen, current, 0x1001C654, 0x2000031C)
            if mutation == "frame":
                objects.sizes[0x1001C654] = 1616
            elif mutation == "pool":
                StartupConfigurationCopy.put(
                    objects, 0x1001C654 + 7124, (0x20000320).to_bytes(4, "little")
                )
            else:
                StartupConfigurationCopy.put(objects, 0x1001C654 + 7108, bytes.fromhex("00bf00bf"))
            self.assertEqual(_parents(frozen, current, 0x1001C654, 0x2000031C), saved)
            fresh, new_rows = _detached_inputs(objects, rows)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                _parents(fresh, new_rows, 0x1001C654, 0x2000031C)

    def test_custom_parent_reader_rechecks_non_site_bytes_without_ELF_metadata_change(self):
        from config_stack import DiagnosticObjects
        from config_decoded_tail import _parents

        class Custom(DiagnosticObjects):
            changed = False
            reads = 0

            def read_only(self, address, extent):
                self.reads += 1
                raw = super().read_only(address, extent)
                site = 0x1001C654 + 7112
                if self.changed and address <= site < address + extent:
                    off = site - address
                    raw = raw[:off] + bytes((raw[off] ^ 1,)) + raw[off + 1 :]
                return raw

        objects, rows, _, _ = TailLiteralBodies.fixture()
        custom = Custom(objects.elf, objects.sizes)
        first = _parents(custom, rows, 0x1001C654, 0x2000031C)
        count = custom.reads
        self.assertEqual(_parents(custom, rows, 0x1001C654, 0x2000031C), first)
        self.assertGreater(custom.reads, count)
        custom.changed = True
        with self.assertRaises(AssertionError):
            _parents(custom, rows, 0x1001C654, 0x2000031C)


class TailPrivateHandoff(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_sources import _first_boot_decoded_sources_state

        objects, rows, source, _ = TailLiteralBodies.fixture(72)
        _, cls.state = _first_boot_decoded_sources_state(objects, rows, source)
        cls.objects = cls.state.phases.objects
        cls.rows = cls.state.phases.rows

    def test_actual_Source_exit_initializes_no_Tail_fields_and_retires_only_saved7(self):
        from config_decoded_tail import _tail_entries
        from config_abstract import Span

        entries = _tail_entries(self.objects, self.rows, self.state)
        self.assertTrue(entries)
        self.assertEqual(len(entries), len(self.state.exits))
        for entry, source in zip(entries, self.state.exits):
            frame = self.state.phases.widgets.views.context.headers["visitor_frame"]
            V = 0x1001C654
            self.assertIs(entry.exit, source)
            self.assertIs(entry.sequence, source.sequence)
            self.assertEqual(entry.sequence.count, Span(4, 4))
            self.assertEqual(entry.frontier, V + 5630)
            self.assertEqual(entry.r4_pointer, (frame.start + 1064, V + 5538, V + 5578))
            self.assertEqual(
                (entry.second_screen_pointer[0].start, entry.second_screen_pointer[1]),
                (frame.start + 216, frame.start + 1001),
            )
            self.assertTrue(
                any(v.start == frame.start + 216 and v.extent == 4 for v in entry.protected)
            )
            self.assertFalse(
                any(
                    v.space == frame.space and v.start == frame.start + 280 and v.extent == 4
                    for v in entry.protected
                )
            )
            self.assertEqual(entry.retired_saved_count[1:3], (7, V + 5052))
            self.assertIsNone(entry.Controls_sequence)
            self.assertEqual((entry.old_controls.offset, entry.old_controls.extent), (212, 32))
            self.assertFalse(entry.completed_ordinary_group_issued)

    def test_copied_public_receipt_or_cloned_Source_exit_is_not_handoff(self):
        from config_decoded_tail import _tail_entries

        with self.assertRaises(AssertionError):
            _tail_entries(self.objects, self.rows, dict(Controls_ready=True))
        first = self.state.exits[0]
        changed = replace(
            self.state,
            exits=(replace(first, source_normal=dict(first.source_normal)),) + self.state.exits[1:],
        )
        with self.assertRaisesRegex(AssertionError, "outgoing state"):
            _tail_entries(self.objects, self.rows, changed)
        with self.assertRaises(AssertionError):
            _tail_entries(self.objects, dict(self.rows), self.state)

    def test_closed_original_Visitor_restore_uses_full96_saved_bytes_and_original_loader_successor(
        self,
    ):
        from config_tail_control import _restore_equations
        from config_decoded_tail import _parents
        from config_tail_children import tail_children
        from llvm_stack import Instruction

        c = self.state.phases.widgets.views.context
        frame = c.headers["visitor_frame"]
        V = c.headers["visitor"]["proof"]["function"]
        parents = _parents(self.objects, self.rows, V, c.owner.start)
        children = tail_children(self.objects, self.rows, parents)
        proof = _restore_equations(self.objects, self.rows, c, parents, children)
        self.assertEqual(proof["release_bytes"], (1512, 56, 4, 36))
        self.assertEqual(proof["original_return_PC"], c.headers["root"]["call_site"] + 4)
        self.assertEqual(proof["return_SP"], frame.start + 1608)
        self.assertEqual(proof["saved_interval"], (frame.start + 1512, frame.start + 1608))
        self.assertEqual(proof["restored_GPRs"], ("r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11"))
        self.assertEqual(proof["restored_FP"], tuple("d" + str(i) for i in range(8, 15)))
        self.assertFalse(proof["architectural_LR_preserved"])
        self.assertFalse(proof["proves_guarded_parent_execution_or_saved_cell_noninterference"])
        for off, op, operand in (
            (62, "vpop", "{d8, d9, d10, d11, d12, d13}"),
            (66, "add", "sp, #0x8"),
        ):
            rows = dict(self.rows)
            row = rows[V + off]
            rows[V + off] = Instruction(row.address, row.raw, op, operand)
            with self.subTest(offset=off), self.assertRaises(AssertionError):
                _restore_equations(self.objects, rows, c, parents, children)


class TailControlsSource(unittest.TestCase):
    @staticmethod
    def fixture(extent=64):
        from config_decoded_tail import _parents
        from config_abstract import SequenceInterface, Span
        from config_regions import borrow
        from config_structural import _detached_inputs

        args = TailOutputAggregates.fixture(extent=extent)
        objects, rows = _detached_inputs(args[0], args[1])
        frame = args[4].storage
        sequence = SequenceInterface(
            borrow(frame, 288, 8, frame.phase, True), args[3].decoder, Span(4, 4)
        )
        return (
            objects,
            rows,
            _parents(objects, rows, 0x1001C654, 0x2000031C),
            sequence,
            frame,
            args[6],
        )

    def test_hold_then_three_Screens_preserve_actual_source_and_all_returned_outputs(self):
        from config_tail_source import _controls_source
        from config_abstract import Span

        result = _controls_source(*self.fixture())
        normal = result["normal"]
        self.assertIsNotNone(normal)
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["invocations"]), (5656, 5712, 6198, 6426)
        )
        self.assertEqual(tuple(v["output"].offset for v in normal["screens"]), (940, 1000, 1064))
        self.assertEqual(normal["after"].count, Span(3, 3))
        self.assertEqual(normal["ControlQ"].count, Span(1, 1))
        self.assertEqual(normal["ScreenQ"].count, Span(0, 0))
        self.assertEqual(normal["after"].decoder.cursor.lower, 31)
        self.assertEqual(normal["hold_domain"], (0, 65535))
        for index, call in enumerate(result["invocations"][1:]):
            self.assertIs(call["before"], call["result"]["before"])
            self.assertEqual(call["before"].count, Span(3 - index, 3 - index))
            retained = {(v.start, v.extent) for v in call["result"]["protected"]}
            self.assertTrue(
                all(
                    (result["invocations"][j]["output"].start, 10) in retained
                    for j in range(1, index + 1)
                )
            )
        self.assertTrue(result["requires_selected_parent_source_control"])
        self.assertEqual(result["Config_writes"], ())

    def test_short_last_Screen_retains_its_error_source_and_never_assigns_Controls(self):
        from config_tail_source import _controls_source

        result = _controls_source(*self.fixture(30))
        self.assertIsNone(result["normal"])
        self.assertEqual(len(result["invocations"]), 4)
        self.assertEqual(result["Config_writes"], ())
        last = result["invocations"][-1]
        self.assertTrue(any(v["origin"] is last for v in result["errors"]))
        self.assertTrue(all(v["origin"] in result["invocations"] for v in result["errors"]))
        self.assertTrue(all(v["cursor"] == v["cause"]["cursor"] for v in result["errors"]))


class TailControlsPublication(unittest.TestCase):
    @staticmethod
    def fixture(extent=64):
        from config_tail_source import _controls_source
        from config_regions import Storage

        args = TailControlsSource.fixture(extent)
        source = _controls_source(*args)
        owner = Storage(
            "address", "supplied same Config", 0x2000031C, 14604, 4, args[4].phase, True
        )
        return args[0], args[1], args[2], source, owner, args[4], args[5]

    def test_all_Screen_original_returns_precede_staging_and_exact_partial_Config_writes(self):
        from config_tail_source import _controls_effects

        result = _controls_effects(*self.fixture())
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["copies"]), (6492, 6518, 6088, 6106)
        )
        self.assertEqual(
            tuple(v["destination"].offset for v in result["copies"]), (516, 628, 223, 233)
        )
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["Config_stages"]),
            (6062, 6066, 6070, 6074, 6078, 6088, 6098, 6106, 6114),
        )
        self.assertEqual(
            tuple(v["completed_screens"] for v in result["Config_stages"]),
            (0, 0, 0, 0, 1, 2, 2, 3, 3),
        )
        self.assertEqual(result["Config_stages"][2]["changed_bytes"], (212, 213, 222))
        self.assertEqual(result["Config_stages"][-1]["changed_bytes"], tuple(range(212, 244)))
        self.assertEqual(result["complete_after"], 0x1001C654 + 6118)
        self.assertEqual(result["requires_bound_first_Screen_word_access_policy"], True)
        self.assertFalse(result["completed_ordinary_group_issued"])
        self.assertEqual(
            tuple(v["instruction"] - 0x1001C654 for v in result["first_only_words"]),
            (6496, 6510, 6074, 6078),
        )

    def test_incomplete_source_and_protected_early_second_rotation_do_not_assign(self):
        from config_tail_source import _controls_effects
        from config_regions import Storage

        args = list(self.fixture(30))
        result = _controls_effects(*args)
        self.assertEqual(result["Config_stages"], ())
        self.assertIsNone(result["complete_after"])
        args = list(self.fixture())
        frame = args[5]
        args[6] += (
            Storage(
                "address",
                "protected early second rotation",
                args[4].start + 222,
                1,
                1,
                frame.phase,
                False,
            ),
        )
        with self.assertRaisesRegex(AssertionError, "protected live storage"):
            _controls_effects(*args)

    def test_unpaired_output_or_original_return_wrong_Config_and_physical_copy_guard_reject(self):
        from config_tail_source import _controls_effects
        from config_regions import Storage

        args = list(self.fixture())
        normal = args[3]["normal"]
        second = normal["screens"][1]
        output = second["output"]
        second["output"] = normal["screens"][0]["output"]
        with self.assertRaisesRegex(AssertionError, "output/source/original-return"):
            _controls_effects(*args)
        second["output"] = output
        node = second["original_return"][0]
        pc = node["return_PC"]
        node["return_PC"] = pc + 2
        with self.assertRaisesRegex(AssertionError, "original return/high-GPR"):
            _controls_effects(*args)
        node["return_PC"] = pc
        self.assertIsNotNone(_controls_effects(*args)["complete_after"])
        saved = args[6]
        frame = args[5]
        args[6] += (
            Storage(
                "address", "physical copy child overlap", frame.start - 72, 1, 1, frame.phase, False
            ),
        )
        with self.assertRaisesRegex(AssertionError, "overlap"):
            _controls_effects(*args)
        args[6] = saved
        args[4] = replace(args[4], start=args[5].start - 152)
        with self.assertRaises(AssertionError):
            _controls_effects(*args)


class TailHistoryLEDSource(unittest.TestCase):
    @staticmethod
    def fixture(extent=64):
        from config_tail_source import _controls_effects

        args = TailControlsPublication.fixture(extent)
        controls = _controls_effects(*args)
        return args[0], args[1], args[2], controls, args[5], args[6]

    def test_history_reduced_and_full_LED_keep_order_domains_and_retired_header_epochs(self):
        from config_tail_source import _history_led_source
        from config_abstract import Span

        result = _history_led_source(*self.fixture())
        normal = result["normal"]
        self.assertIsNotNone(normal)
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["history"]["invocations"]),
            (6144, 6170, 6460, 6534, 6562, 6590, 6616, 6642),
        )
        self.assertEqual(result["history"]["normal"]["after"].count, Span(0, 0))
        self.assertEqual(result["reduced"]["normal"]["after"].count, Span(1, 1))
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["LED"]["invocations"]),
            (6792, 6828, 6876, 6900, 6926, 6952, 6978),
        )
        self.assertEqual(normal["after"].count, Span(1, 1))
        self.assertEqual(normal["LEDQ"].count, Span(0, 0))
        self.assertEqual(normal["after"].decoder.cursor.lower, 55)
        self.assertEqual(normal["physical_top_count"], 1)
        self.assertFalse(normal["old_history_Q_still_live"])
        for call in result["LED"]["invocations"][:2]:
            self.assertEqual(call["result"]["normal"]["fields"][2], ("brightness", (0, 255)))
            self.assertEqual(call["result"]["normal"]["fields"][3], ("period_ms", (0, 65535)))
        self.assertTrue(result["requires_selected_parent_source_control"])

    def test_short_LED_keeps_completed_history_reduced_and_exact_failed_child_origin(self):
        from config_tail_source import _history_led_source

        result = _history_led_source(*self.fixture(50))
        self.assertIsNone(result["normal"])
        self.assertIsNotNone(result["history"]["normal"])
        self.assertIsNotNone(result["reduced"]["normal"])
        self.assertIsNone(result["LED"]["normal"])
        self.assertTrue(
            any(
                v["completed_groups"] == ("Controls", "history", "reduced_motion")
                for v in result["errors"]
            )
        )
        self.assertTrue(
            all(
                any(v["origin"] is call for call in result["invocations"]) for v in result["errors"]
            )
        )

    def test_bool_actual_pop14_and_all_conditional_returns_keep_original_PC_not_architectural_LR(
        self,
    ):
        from config_tail_children import tail_children, tail_return_profiles
        from config_finite_returns import _closed_child_returns
        from llvm_stack import Instruction

        objects, rows, parents, _, frame, _ = self.fixture()
        children = tail_children(objects, rows, parents)
        edge = next(
            e
            for p in parents
            for e in p["ordinary_edges"]
            if p["function"] + e[0] == 0x1001C654 + 6736
        )
        proof = children[edge[2], edge[3]]
        start = proof["function"]
        pc = 0x1001C654 + 6740
        parent = dict(function=pc - 4, ordinary_edges=((0, "bl", start, edge[3], 8),))
        nodes = _closed_child_returns(
            rows, (parent,), children, frame.start, tail_return_profiles(), (), objects=objects
        )
        node = next(v for v in nodes if v["function"] == start and v["return_PC"] == pc)
        self.assertEqual({at - start for at, returned, sp in node["returns"]}, {22, 40, 46, 52})
        self.assertTrue(
            all(returned == pc and sp == frame.start for at, returned, sp in node["returns"])
        )
        self.assertFalse(node["architectural_LR_preserved"])
        # Private walker mutation only: complete literal validation is a
        # separate prerequisite, and no altered machine image is claimed.
        changed = dict(rows)
        row = changed[start + 14]
        changed[start + 14] = Instruction(row.address, row.raw, row.mnemonic, "{r7, r4}")
        with self.assertRaisesRegex(AssertionError, "another caller register"):
            _closed_child_returns(
                changed, (parent,), children, frame.start, tail_return_profiles(), ()
            )


class TailSuffixPublication(unittest.TestCase):
    @staticmethod
    def fixture(extent=64):
        from config_tail_source import _history_led_source

        args = TailHistoryLEDSource.fixture(extent)
        return args[0], args[1], args[2], _history_led_source(*args), args[4], args[5]

    def test_ordered_history_reduced_and_all_late_LED_stores_preserve_padding(self):
        from config_tail_source import _suffix_effects

        result = _suffix_effects(*self.fixture())
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["history"]),
            (6672, 6680, 6690, 6698, 6706, 6714, 6722, 6728, 6732),
        )
        self.assertEqual(result["reduced"]["site"], 0x1001C654 + 6768)
        self.assertEqual(
            tuple(v["site"] - 0x1001C654 for v in result["LED"]),
            (7026, 7034, 7038, 7046, 7054, 7062, 7068, 7076, 7082, 7088, 7096, 7102, 7108),
        )
        self.assertFalse(result["LED"][-2]["complete_LED"])
        self.assertTrue(result["LED"][-1]["complete_LED"])
        self.assertEqual(result["complete_after"], 0x1001C654 + 7112)
        self.assertEqual(result["preserved_LED_padding"], (14569,))
        self.assertEqual(
            result["written_opaque_LED_bytes"], (14545, 14549, 14553, 14555, 14559, 14563)
        )
        self.assertNotIn(14569, result["LED"][-1]["changed_bytes"])
        self.assertEqual(result["early_success_register"], 0x1001C654 + 7100)
        self.assertFalse(result["completed_ordinary_group_issued"])

    def test_later_LED_failure_keeps_assigned_history_and_reduced_without_LED_writes(self):
        from config_tail_source import _suffix_effects

        result = _suffix_effects(*self.fixture(50))
        self.assertEqual(len(result["history"]), 9)
        self.assertIsNotNone(result["reduced"])
        self.assertEqual(result["LED"], ())
        self.assertIsNone(result["complete_after"])


class TailPrivatePhases(unittest.TestCase):
    def test_actual_private_Source_producer_once_feeds_all_current_Tail_phases(self):
        from config_decoded_tail import _first_boot_tail_phases
        from config_abstract import Span

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        state = _first_boot_tail_phases(objects, rows, source)
        self.assertTrue(state.entries)
        self.assertEqual(len(state.entries), len(state.predecessor.exits))
        self.assertEqual(len(state.phases), len(state.entries))
        normals = tuple(
            v
            for v in state.phases
            if v.suffix_effects is not None and v.suffix_effects["complete_after"] is not None
        )
        self.assertTrue(normals)
        for phase, entry in zip(state.phases, state.entries):
            self.assertIs(phase.entry, entry)
            self.assertTrue(any(entry.exit is v for v in state.predecessor.exits))
            self.assertIs(phase.controls["before"], entry.sequence)
            self.assertEqual(phase.parent_envelopes["stores"], 57)
            self.assertFalse(phase.completed_ordinary_group_issued)
        for phase in normals:
            self.assertEqual(phase.suffix["normal"]["after"].count, Span(1, 1))
            self.assertEqual(phase.suffix_effects["complete_after"], 0x1001C654 + 7112)
        self.assertIs(state.current(), state)
        old = normals[0].controls["top_decrement"]
        normals[0].controls["top_decrement"] = (old[0], Span(4, 4), Span(0, 0))
        with self.assertRaisesRegex(
            AssertionError, "Tail private source/epoch/child/effect record changed"
        ):
            state.current()
        normals[0].controls["top_decrement"] = old
        self.assertIs(state.current(), state)
        self.assertFalse(state.completed_ordinary_group_issued)

    def test_absent_private_Source_exit_does_not_construct_Tail_source_or_state(self):
        from config_decoded_tail import _first_boot_tail_phases
        from unittest.mock import patch
        import config_tail_source

        objects, rows, source, _ = TailLiteralBodies.fixture(1)
        with patch.object(
            config_tail_source, "_controls_source", wraps=config_tail_source._controls_source
        ) as source_call:
            state = _first_boot_tail_phases(objects, rows, source)
        self.assertEqual(state.entries, ())
        self.assertEqual(state.phases, ())
        self.assertEqual(source_call.call_count, 0)
        self.assertIs(state.current(), state)


class TailWholeStructure(unittest.TestCase):
    @staticmethod
    def fixture(mutation=None):
        from config_decoded_tail import _first_boot_tail_phases
        from test_config_stack import (
            StartupConfigurationCopy,
            BoundedParentTargets,
            SameBLoaderBoundary,
        )
        from llvm_stack import Instruction

        objects, rows, source, _ = TailLiteralBodies.fixture(1)
        if mutation == "foreign":
            site = 0x1002C000
            target = 0x1001C654 + 5632
            raw = BoundedParentTargets.conditional_branch(site, target, 0)
            StartupConfigurationCopy.put(objects, site, raw)
            rows[site] = Instruction(site, raw, "beq.w", hex(target))
        elif mutation == "padding":
            # New reachable code replaces the exact terminal NOP and points
            # into the live return path. Source's earlier frontier ignores
            # this unentered location; full Tail must classify it itself.
            site = 0x1001C654 + 7122
            target = 0x1001C654 + 6988
            import struct

            raw = struct.pack("<H", 0xE000 | (((target - (site + 4)) // 2) & 0x7FF))
            StartupConfigurationCopy.put(objects, site, raw)
            rows[site] = Instruction(site, raw, "b", hex(target))
        if mutation:
            source = SameBLoaderBoundary.rebound(objects, source)
        return _first_boot_tail_phases(objects, rows, source)

    def test_complete_current_Visitor_selection_consumes_old_frontiers_and_keeps_only_dormant4408(
        self,
    ):
        from config_tail_control import _tail_structural

        state = self.fixture()
        raw = _tail_structural(state)
        V = 0x1001C654
        self.assertIn(V + 54, raw.selection.selected)
        self.assertIn(V + 56, raw.selection.selected)
        self.assertIn(V + 6654, raw.selection.selected)
        self.assertIn(V + 7108, raw.selection.selected)
        self.assertNotIn(V + 7122, raw.selection.selected)
        self.assertNotIn(V + 7124, raw.selection.selected)
        self.assertEqual(
            tuple(at - V for at, target, reason in raw.outside_shared_contexts), (4408,)
        )
        self.assertEqual(raw.terminal_partition["padding"], (V + 7122, V + 7124))
        self.assertEqual(raw.carrier_returns["parent_call_count"], 25)
        self.assertFalse(raw.confers_active_state)
        self.assertFalse(raw.restore["successor_executed"])

    def test_current_foreign_conditional_ingress_and_reachable_terminal_padding_reject(self):
        from config_tail_control import _tail_structural

        for mutation, expected in (
            ("foreign", "foreign direct ingress"),
            ("padding", "terminal padding"),
        ):
            with self.subTest(mutation=mutation), self.assertRaisesRegex(AssertionError, expected):
                _tail_structural(self.fixture(mutation))


class TailRestoreContexts(unittest.TestCase):
    def test_current_RenderTrace_restore_context_relocates_with_its_local_conditional_target(self):
        # Real retained caller bytes/metadata/listing, supplied only as an
        # unresolved foreign context. Relocation cannot grant its save stack.
        from pathlib import Path
        import json
        from llvm_stack import Instruction
        from config_tail_control import _restore_ingress
        from test_config_stack import ActualDecodedViews, StartupConfigurationCopy

        f = json.loads(
            (Path(__file__).parent / "fixtures/config-tail-restore-context.json").read_bytes()
        )
        objects, rows, _, _ = TailLiteralBodies.fixture(1)
        name = f["name"]
        role = f["role"]
        at = f["address"]
        extent = f["extent"]
        (section,) = tuple(
            i
            for i, s in enumerate(objects.elf.sections)
            if s[1] == 1 and s[3] <= at < at + extent <= s[3] + s[5]
        )
        objects.elf.functions[name] = (at | 1, extent, section)
        objects.elf.symbol_entries.append((name, at | 1, extent, 2, section))
        objects.sizes[at] = f["frame"]
        objects.elf.mapping_symbols.extend((n, a, section) for n, a, i in f["mapping"])
        StartupConfigurationCopy.put(objects, at, bytes.fromhex(f["bytes"]))
        for r in f["records"]:
            rows[r["address"]] = Instruction(
                r["address"], bytes.fromhex(r["bytes"]), r["mnemonic"], r["operands"]
            )
        restore = dict(restore=dict(function=0x100289F0), epilogue=dict(function=0x1001C654 + 56))
        before = _restore_ingress(objects, rows, restore)
        self.assertEqual(len(before), 1)
        self.assertEqual(before[0]["entry_site"], at + 60)
        ActualDecodedViews.move_function(objects, rows, role, 0x10071000)
        after = _restore_ingress(objects, rows, restore)
        self.assertEqual(len(after), 1)
        self.assertEqual(after[0]["entry_site"], 0x10071000 + 60)
        self.assertEqual(
            rows[0x10071000 + 54].operands.split("<", 1)[0].strip(), hex(0x10071000 + 64)
        )
        self.assertTrue(after[0]["inherited_source_stack_and_return_unresolved"])
        self.assertFalse(after[0]["selected_Visitor_permission"])
        # Coherent changed local conditional target remains a different
        # unresolved caller context, rather than borrowing its old anchor.
        import struct
        from llvm_stack import Instruction

        at = 0x10071000 + 54
        target = 0x10071000 + 66
        old = rows[at]
        raw = struct.pack("<H", 0xD200 | (((target - (at + 4)) // 2) & 255))
        StartupConfigurationCopy.put(objects, at, raw)
        rows[at] = Instruction(at, raw, "bhs", hex(target))
        with self.assertRaisesRegex(AssertionError, "local branch|caller context"):
            _restore_ingress(objects, rows, restore)


class TailParentLineage(unittest.TestCase):
    @staticmethod
    def fixture(extent=64):
        from config_tail_source import (
            _controls_source,
            _controls_effects,
            _history_led_source,
            _suffix_effects,
        )
        from config_tail_children import tail_children, tail_return_profiles
        from config_finite_returns import _closed_child_returns
        from config_tail_contracts import ROLES

        args = TailControlsPublication.fixture(extent)
        objects, rows, parents, controls, owner, frame, protected = args
        effects = _controls_effects(*args)
        suffix = (
            _history_led_source(objects, rows, parents, effects, frame, protected)
            if effects["complete_after"]
            else None
        )
        writes = (
            _suffix_effects(objects, rows, parents, suffix, frame, protected)
            if suffix is not None
            else None
        )
        children = tail_children(objects, rows, parents)
        callable_parents = tuple(
            dict(
                function=p["function"],
                ordinary_edges=tuple(e for e in p["ordinary_edges"] if e[1] == "bl"),
            )
            for p in parents
            if any(e[1] == "bl" for e in p["ordinary_edges"])
        )
        returns = _closed_child_returns(
            rows,
            callable_parents,
            children,
            frame.start,
            tail_return_profiles(),
            (ROLES["ControlAction"], ROLES["ButtonBinding"], ROLES["LedPattern"]),
            objects=objects,
        )
        return objects, rows, parents, children, returns, controls, effects, suffix, writes, frame

    def test_conditional_normal_path_pairs_all_source_returns_late_stores_and_carrier_lifetimes(
        self,
    ):
        from config_tail_control import _parent_lineage

        args = self.fixture()
        result = _parent_lineage(*args)
        V = 0x1001C654
        self.assertEqual(result["normal"]["frontier"], V + 7112)
        self.assertEqual(result["normal"]["return_carrier"], 255)
        self.assertEqual(result["normal"]["physical_top_count"], 1)
        self.assertEqual(result["normal"]["logical_remaining_fields"], 0)
        self.assertEqual(result["normal"]["path"][-1], V + 7112)
        self.assertLess(
            result["normal"]["path"].index(V + 7100), result["normal"]["path"].index(V + 7108)
        )
        self.assertEqual(len(result["normal"]["calls"]), 25)
        self.assertTrue(result["errors"])
        self.assertTrue(
            all(v["path"][-1] == V + 56 and v["source_error"] is not None for v in result["errors"])
        )
        self.assertFalse(result["confers_active_state"])

    def test_wrong_source_error_late_store_and_register_transfer_cannot_authorize_parent_completion(
        self,
    ):
        from config_tail_control import _parent_lineage

        args = list(self.fixture())
        before = args[7]["invocations"][0]["before"]
        args[7]["invocations"][0]["before"] = replace(before, count=before.count.__class__(0, 0))
        with self.assertRaises(AssertionError):
            _parent_lineage(*args)
        args[7]["invocations"][0]["before"] = before
        complete = args[8]["complete_after"]
        args[8]["complete_after"] = 0x1001C654 + 7100
        with self.assertRaisesRegex(AssertionError, "late|last|complete"):
            _parent_lineage(*args)
        args[8]["complete_after"] = complete
        node = next(v for v in args[4] if v["return_PC"] == 0x1001C654 + 6828 + 4)
        high = node["preserved_high_gprs"]
        node["preserved_high_gprs"] = high[:-1]
        with self.assertRaisesRegex(AssertionError, "carrier|register|GPR"):
            _parent_lineage(*args)
        node["preserved_high_gprs"] = high
        error = args[5]["errors"][0]
        cause = error["cause"]
        error["cause"] = dict(cause)
        with self.assertRaisesRegex(AssertionError, "error.*origin|origin.*error"):
            _parent_lineage(*args)
        error["cause"] = cause
        self.assertEqual(_parent_lineage(*args)["normal"]["frontier"], 0x1001C654 + 7112)

    def test_short_LED_error_retains_previous_groups_without_success_carrier(self):
        from config_tail_control import _parent_lineage

        args = self.fixture(50)
        result = _parent_lineage(*args)
        self.assertIsNone(result["normal"])
        self.assertTrue(result["errors"])
        late = [
            v
            for v in result["errors"]
            if v["completed_groups"] == ("Controls", "history", "reduced_motion")
        ]
        self.assertTrue(late)
        self.assertTrue(all(v["return_carrier"] != 255 for v in late))
        self.assertTrue(all(not v["LED_Config_writes"] for v in late))


class TailActualActiveControl(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_tail import _first_boot_tail_phases

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        cls.state = _first_boot_tail_phases(objects, rows, source)

    def test_actual_private_predecessor_binds_guards_saved_cells_and_original_return(self):
        from config_tail_control import _tail_control

        control = _tail_control(self.state)
        V = 0x1001C654
        self.assertIs(control.state, self.state)
        self.assertEqual(len(control.guards), len(self.state.entries))
        self.assertTrue(any(v["lineage"]["normal"] is not None for v in control.guards))
        self.assertEqual(control.record["return"]["original_return_PC"], 0x1001FE64 + 130)
        self.assertEqual(control.record["return"]["release_bytes"], (1512, 56, 4, 36))
        self.assertEqual(control.record["outside_shared_contexts"][0][0], V + 4408)
        self.assertFalse(control.record["successor_executed"])
        self.assertFalse(control.record["exception_safe_completion"])
        self.assertTrue(control.record["actual_reached_accesses_pending"])
        normal = next(v for v in control.guards if v["lineage"]["normal"] is not None)
        self.assertIs(normal["entry"], self.state.entries[normal["entry_index"]])
        saved = normal["saved_interval"]
        self.assertEqual(
            saved,
            (
                self.state.phases[0].entry.sequence.header.storage.start + 1512,
                self.state.phases[0].entry.sequence.header.storage.start + 1608,
            ),
        )
        old = normal["lineage"]["normal"]["return_carrier"]
        normal["lineage"]["normal"]["return_carrier"] = 0
        with self.assertRaisesRegex(AssertionError, "Tail active source/control record changed"):
            control.current()
        normal["lineage"]["normal"]["return_carrier"] = old
        origin = normal["Source_normal"]
        normal["Source_normal"] = dict(origin)
        with self.assertRaisesRegex(AssertionError, "exact entry/normal association"):
            control.current()
        normal["Source_normal"] = origin
        self.assertIs(control.current(), control)
        import config_platform

        receipt = self.state.predecessor.phases.widgets.execution
        with self.assertRaisesRegex(AssertionError, "exception-safe"):
            config_platform.require_exception_safe_execution(receipt)
        with self.assertRaisesRegex(AssertionError, "global ownership"):
            config_platform.require_global_execution_owner(receipt)
        with self.assertRaises(AssertionError):
            _tail_control(dict(phase="Tail complete", complete=True))

    def test_current_rejects_each_retained_predecessor_alias_before_derived_consumers(self):
        from config_tail_control import _tail_control

        control = _tail_control(self.state)
        selection = control.structural.selection
        replacements = dict(
            context=replace(selection.context),
            source_selection=replace(selection.source_selection),
            widgets=replace(selection.widgets),
            Source_parents=tuple(list(selection.Source_parents)),
            Source_children=dict(selection.Source_children),
        )
        for field, value in replacements.items():
            old = getattr(selection, field)
            self.assertIsNot(old, value)
            object.__setattr__(selection, field, value)
            try:
                with (
                    self.subTest(alias=field),
                    self.assertRaisesRegex(AssertionError, "association|structure|retained"),
                ):
                    control.current()
            finally:
                object.__setattr__(selection, field, old)
        object.__setattr__(control, "actual_reached_accesses_pending", False)
        try:
            with self.assertRaisesRegex(AssertionError, "retained reached-access qualification"):
                control.current()
        finally:
            object.__setattr__(control, "actual_reached_accesses_pending", True)
        self.assertIs(control.current(), control)
        self.assertIs(
            control.structural.selection.context,
            self.state.predecessor.phases.widgets.views.context,
        )


class TailReachedAccesses(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_tail_uses import _tail_access_group
        from config_decoded_tail import _first_boot_tail_phases

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        cls.state = _first_boot_tail_phases(objects, rows, source)
        cls.bridge, cls.uses = _tail_access_group(cls.state)

    def test_actual_three_Screen_epochs_first_only_and_both_LED_sources_reach_current_requirements(
        self,
    ):
        from config_tail_uses import _check_tail_uses
        from collections import Counter

        self.assertIs(self.uses, self.bridge.uses)
        self.assertEqual(
            tuple(v.role for v in self.bridge.tail_families),
            ("Screen_output_words", "first_Screen_scratch_words", "first_Config_Screen_words"),
        )
        self.assertEqual(
            tuple(v.role for v in self.bridge.LED_families),
            ("result_words", "pattern_words", "normal_config_words"),
        )
        counts = Counter(v.role for v in self.uses)
        self.assertTrue(
            all(
                counts[v] > 0
                for v in (
                    "Screen_output_words",
                    "first_Screen_scratch_words",
                    "first_Config_Screen_words",
                    "result_words",
                    "pattern_words",
                    "normal_config_words",
                )
            )
        )
        for use in self.uses:
            phase = self.state.phases[use.phase_index]
            self.assertIs(use.entry, phase.entry)
            self.assertFalse(use.completed_prefix_issued)
            if use.role == "Screen_output_words":
                self.assertEqual(
                    use.epoch,
                    (5712, 6198, 6426).index(use.source["invocation"]["site"] - 0x1001C654),
                )
                self.assertIs(use.source["normal"], use.source["invocation"]["result"]["normal"])
            elif use.role in ("first_Screen_scratch_words", "first_Config_Screen_words"):
                self.assertIs(use.source["Screen"], phase.controls["normal"]["screens"][0])
                self.assertEqual(use.epoch, 0)
            elif use.role == "normal_config_words":
                self.assertIsNotNone(phase.suffix_effects["complete_after"])
                self.assertEqual(use.continuation[-1], 0x1001C654 + 7112)
        self.assertIs(_check_tail_uses(self.bridge), self.uses)

    def test_equal_copy_source_and_late_completion_mutation_reject_whole_current_collection(self):
        from config_tail_uses import _check_tail_uses

        use = next(v for v in self.uses if v.role == "Screen_output_words")
        original = use.source["invocation"]
        use.source["invocation"] = dict(original)
        try:
            with self.assertRaisesRegex(AssertionError, "Tail reached access collection changed"):
                _check_tail_uses(self.bridge)
        finally:
            use.source["invocation"] = original
        phase = next(
            v
            for v in self.state.phases
            if v.suffix_effects is not None and v.suffix_effects["complete_after"] is not None
        )
        original = phase.suffix_effects["complete_after"]
        phase.suffix_effects["complete_after"] = 0x1001C654 + 7100
        try:
            with self.assertRaises(AssertionError):
                _check_tail_uses(self.bridge)
        finally:
            phase.suffix_effects["complete_after"] = original

    def test_public_descriptors_or_changed_collection_cannot_authorize_current_accesses(self):
        from config_tail_uses import _tail_access_group, _check_tail_uses

        for value in (dict(phase="Tail complete"), self.bridge.tail_families[0], None):
            with self.assertRaises(AssertionError):
                _tail_access_group(value)
        with self.assertRaises(AssertionError):
            _check_tail_uses(dict(uses=self.uses, verified=True))
        # Derived value wrappers may be independently recreated. Their actual
        # source/entry/control aliases remain checked by the fresh batch.
        copies = tuple(replace(v) for v in self.uses)
        self.assertIs(_check_tail_uses(replace(self.bridge, uses=copies)), copies)
        for changed in (self.uses[:-1], tuple(reversed(self.uses)), self.uses + (self.uses[0],)):
            with (
                self.subTest(change=len(changed)),
                self.assertRaisesRegex(AssertionError, "collection changed"),
            ):
                _check_tail_uses(replace(self.bridge, uses=changed))


class TailPublicErrorProjection(unittest.TestCase):
    def test_prior_occurrences_and_repeated_Tail_labels_are_conserved_once(self):
        from types import SimpleNamespace
        from config_abstract import Span
        from config_decoded_tail import _tail_error_summary

        prior = dict(
            error_summary=(("prefix", "version", 14, -1, 56, (), 3, 0, 2),), error_occurrences=3
        )
        first = dict(field="normal_pattern", error=4, cursor=Span(50, 60))
        second = dict(first, cursor=Span(55, 65))
        completed = ("Controls", "history", "reduced_motion")
        edges = tuple(
            dict(source_error=e, path=(6988, 56), completed_groups=completed)
            for e in (first, second)
        )
        control = SimpleNamespace(guards=(dict(lineage=dict(errors=edges)),))
        result, total = _tail_error_summary(prior, control)
        self.assertEqual(total, 5)
        self.assertEqual(result[0], prior["error_summary"][0])
        self.assertEqual(result[1][:6], ("Tail", "normal_pattern", 4, -1, 56, completed))
        self.assertEqual(result[1][-3:], (2, 50, 65))
        self.assertEqual(_tail_error_summary(prior, None), (prior["error_summary"], 3))
        self.assertEqual(first["cursor"], Span(50, 60))
        with self.assertRaisesRegex(AssertionError, "count changed"):
            _tail_error_summary(dict(prior, error_occurrences=4), control)

    def test_supplied_projection_padding_checks_exact_65536_and_65537_boundary(self):
        # Explicit diagnostic stage fixture, not an actual producer/maximum.
        import json
        from types import SimpleNamespace as N
        from config_decoded_tail import _tail_description

        prior = dict(
            error_summary=(),
            error_occurrences=0,
            error_columns=(),
            ordinary_domain="fixture",
            execution_scope={},
            source_context={},
            source_qualification={},
            external_qualifications={"padding": ""},
            exception_graph_limit={},
        )
        c = N(
            headers={
                "visitor": {"proof": {"function": 0x1001C654}},
                "root": {"proof": {"function": 0x1001FE64}},
            }
        )
        state = N(
            predecessor_description=prior,
            predecessor=N(phases=N(widgets=N(views=N(context=c)))),
            entries=(),
        )
        raw = N(selection=N(selected={}), outside_shared_contexts=(), foreign_restore_contexts=())
        size = lambda value: len((json.dumps(value, sort_keys=True) + "\n").encode())
        baseline = _tail_description(state, None, (), (), raw)
        prior["external_qualifications"]["padding"] = "x" * (65536 - size(baseline))
        self.assertEqual(size(_tail_description(state, None, (), (), raw)), 65536)
        prior["external_qualifications"]["padding"] += "x"
        with self.assertRaisesRegex(AssertionError, "oversized"):
            _tail_description(state, None, (), (), raw)


class ActualDecodedVisitorReturn(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_tail import _first_boot_decoded_visitor_return_state

        cls.objects, cls.rows, cls.source, _ = TailLiteralBodies.fixture(128)
        cls.result, cls.state = _first_boot_decoded_visitor_return_state(
            cls.objects, cls.rows, cls.source
        )

    def test_actual_same_input_normal_return_retires_Visitor_borrows_before_loader(self):
        import json

        result = self.result
        state = self.state
        prior = state.phases.predecessor_description
        self.assertEqual(
            result["phase"],
            "first_boot_decoded_visitor_return_pending_finalize_canonical_validation_and_loader_routes",
        )
        self.assertTrue(result["successes"])
        self.assertTrue(state.exits)
        self.assertEqual(result["Visitor_return"]["frontier"], 0x1001FE64 + 130)
        self.assertTrue(result["Visitor_return"]["Visitor_local_borrows_retired"])
        self.assertFalse(result["Visitor_return"]["loader_successor_executed"])
        self.assertEqual(result["Visitor_return"]["original_saved_bytes"], 96)
        for exit in state.exits:
            phase = state.phases.phases[exit.phase_index]
            self.assertIs(exit.decoder, phase.suffix["normal"]["after"].decoder)
            self.assertIs(exit.source_normal, phase.suffix["normal"])
            self.assertEqual((exit.decoder.header.offset, exit.decoder.header.extent), (236, 8))
            self.assertEqual(exit.original_return[-1], 0x1001FE64 + 130)
        self.assertLessEqual(len((json.dumps(result, sort_keys=True) + "\n").encode()), 65536)
        for key in (
            "execution_scope",
            "ordinary_domain",
            "source_context",
            "source_qualification",
            "external_qualifications",
            "exception_graph_limit",
        ):
            self.assertEqual(
                json.loads(json.dumps(result[key])), json.loads(json.dumps(prior[key]))
            )
        self.assertEqual(
            result["error_summary"][: len(prior["error_summary"])], prior["error_summary"]
        )
        self.assertEqual(sum(v[-3] for v in result["error_summary"]), result["error_occurrences"])
        self.assertTrue(all(n > 0 for role, n in result["source_guarded_accesses"]["role_counts"]))
        self.assertFalse(result["unconditional_owner"])
        self.assertFalse(result["hardware_qualified"])
        self.assertFalse(result["exceptional_resume_established"])

    def test_outgoing_normal_identity_and_late_required_use_cannot_be_replaced(self):
        state = self.state
        old = state.exits
        object.__setattr__(
            state, "exits", (replace(old[0], source_normal=dict(old[0].source_normal)),) + old[1:]
        )
        try:
            with self.assertRaisesRegex(AssertionError, "exact source/normal/original-return"):
                state.current()
        finally:
            object.__setattr__(state, "exits", old)
        bridge = state.access_bridge
        uses = bridge.uses
        object.__setattr__(
            bridge, "uses", tuple(v for v in uses if v.role != "normal_config_words")
        )
        try:
            with self.assertRaisesRegex(AssertionError, "collection changed"):
                state.current()
        finally:
            object.__setattr__(bridge, "uses", uses)

    def test_changed_ELF_cannot_reuse_old_SourceContext_or_a_public_description(self):
        import copy
        from config_decoded_tail import first_boot_decoded_visitor_return
        from test_config_stack import StartupConfigurationCopy

        objects = copy.deepcopy(self.objects)
        StartupConfigurationCopy.put(objects, 0x1001C654 + 7108, bytes.fromhex("aff30080"))
        with self.assertRaisesRegex(AssertionError, "different current ELF"):
            first_boot_decoded_visitor_return(objects, self.rows, self.source)
        with self.assertRaises(AssertionError):
            first_boot_decoded_visitor_return(self.objects, self.rows, self.result)


class UnenteredDecodedVisitorReturn(unittest.TestCase):
    def test_prior_error_only_path_has_no_fabricated_Tail_use_return_or_owner(self):
        from unittest.mock import patch
        import config_decoded_tail as subject

        objects, rows, source, _ = TailLiteralBodies.fixture(1)
        with patch(
            "config_tail_source._controls_source",
            side_effect=AssertionError("fabricated Tail call"),
        ):
            result, state = subject._first_boot_decoded_visitor_return_state(objects, rows, source)
        self.assertFalse(state.phases.entries)
        self.assertIsNone(state.access_bridge)
        self.assertFalse(state.exits)
        self.assertTrue(state.error_exits)
        self.assertTrue(result["Visitor_return"]["original_saved_PC"])
        self.assertEqual(result["Visitor_return"]["normal_returns"], 0)
        self.assertEqual(result["Visitor_return"]["error_returns"], len(state.error_exits))
        self.assertTrue(result["declared_return_frontier"]["reached"])
        self.assertFalse(result["source_guarded_accesses"]["role_counts"])
        prior = state.phases.predecessor_description
        self.assertEqual(result["error_summary"], prior["error_summary"])
        self.assertEqual(result["error_occurrences"], prior["error_occurrences"])
        self.assertTrue(result["structural_control"]["unentered"])
        self.assertIs(state.current(), state)
        original = state.error_exits
        object.__setattr__(
            state,
            "error_exits",
            (replace(original[0], carrier=original[0].carrier + 1),) + original[1:],
        )
        try:
            with self.assertRaisesRegex(AssertionError, "outgoing error.*origin/carrier"):
                state.current()
        finally:
            object.__setattr__(state, "error_exits", original)
        value = prior["hardware_qualified"]
        prior["hardware_qualified"] = not value
        try:
            with self.assertRaisesRegex(AssertionError, "changed"):
                state.current()
        finally:
            prior["hardware_qualified"] = value
        with patch("config_decoded_tail._earlier_error_returns", return_value=()):
            with self.assertRaisesRegex(AssertionError, "actual error occurrence"):
                subject._first_boot_decoded_visitor_return_state(objects, rows, source)


class TailPrivateNormalReturn(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from config_decoded_tail import _tail_exits, _first_boot_tail_phases
        from config_tail_uses import _tail_access_group

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        cls.phases = _first_boot_tail_phases(objects, rows, source)
        cls.bridge, cls.uses = _tail_access_group(cls.phases)

    def test_checked_private_normal_returns_keep_only_caller_D_and_original_PC_SP(self):
        from config_decoded_tail import _tail_exits

        exits = _tail_exits(self.phases, self.bridge, self.uses)
        self.assertTrue(exits)
        c = self.phases.predecessor.phases.widgets.views.context
        for exit in exits:
            phase = self.phases.phases[exit.phase_index]
            self.assertIs(exit.decoder, phase.suffix["normal"]["after"].decoder)
            self.assertIs(exit.source_normal, phase.suffix["normal"])
            self.assertIs(exit.decoder.header.storage, c.frame)
            self.assertEqual((exit.decoder.header.offset, exit.decoder.header.extent), (236, 8))
            self.assertEqual(exit.original_return[-1], 0x1001FE64 + 130)
            self.assertEqual(self.bridge.control.structural.restore["return_SP"], c.frame.start)
        with self.assertRaises(AssertionError):
            _tail_exits(self.phases, dict(uses=self.uses), self.uses)

    def test_missing_late_use_or_wrong_saved_return_cannot_issue_normal_exit(self):
        from config_decoded_tail import _tail_exits

        missing = tuple(u for u in self.uses if u.role != "normal_config_words")
        bridge = replace(self.bridge, uses=missing)
        with self.assertRaisesRegex(AssertionError, "late normal Config"):
            _tail_exits(self.phases, bridge, missing)
        restore = self.bridge.control.structural.restore
        old = restore["original_return_PC"]
        restore["original_return_PC"] = old + 2
        try:
            with self.assertRaisesRegex(AssertionError, "saved-PC/SP"):
                _tail_exits(self.phases, self.bridge, self.uses)
        finally:
            restore["original_return_PC"] = old


class PrefixPrivateErrorFrontiers(unittest.TestCase):
    def test_both_actual_wrapper_positions_retain_origin_cursor_partial_state_and_saved_cells(self):
        from config_decoded_prefix import _first_boot_decoded_u16_state

        seen = set()
        for extent in (1, 4):
            objects, rows, source, _ = TailLiteralBodies.fixture(extent)
            description, context = _first_boot_decoded_u16_state(objects, rows, source)
            errors = context._checked_errors()
            self.assertEqual(
                len(errors), sum(v["error"] is not None for v in description["outcomes"])
            )
            for error in errors:
                seen.add(error.site - 0x1001C654)
                self.assertIs(error.after, error.outcome["state"])
                self.assertIs(error.outcome["inner"]["state"], error.after)
                self.assertEqual(error.packed_r0, (error.outcome["error"] << 8) | 1)
                self.assertEqual(error.frontier, 0x1001C654 + 54)
                self.assertEqual(error.saved.extent, 96)
                self.assertEqual(error.saved.start, context.headers["visitor_frame"].start + 1512)
                self.assertIs(error.before.decoder.header, error.after.decoder.header)
                self.assertIs(error.before.decoder.payload, error.after.decoder.payload)
                self.assertEqual(error.before.count.lower - error.after.count.lower, 1)
            old = context.error_frontiers
            object.__setattr__(context, "error_frontiers", old[:-1])
            try:
                with self.assertRaisesRegex(AssertionError, "error.*changed"):
                    context._checked_errors()
            finally:
                object.__setattr__(context, "error_frontiers", old)
            error = errors[0]
            old = error.packed_r0
            object.__setattr__(error, "packed_r0", old ^ 0x100)
            try:
                with self.assertRaisesRegex(AssertionError, "error.*changed|carrier"):
                    context._checked_errors()
            finally:
                object.__setattr__(error, "packed_r0", old)
        self.assertEqual(seen, {24, 46})


class ViewsPrivateErrorFrontiers(unittest.TestCase):
    def test_actual_error_frontiers_preserve_source_origins_advanced_strings_and_partial_views(
        self,
    ):
        from config_decoded_views import _first_boot_decoded_views_state

        stages = set()
        for extent in (4, 24):
            objects, rows, source, _ = TailLiteralBodies.fixture(extent)
            description, state = _first_boot_decoded_views_state(objects, rows, source)
            frontiers = state._checked_errors()
            self.assertEqual(len(frontiers), len(description["group_errors"]))
            D = state.context.headers["visitor"]["sequence_state"].decoder
            for error in frontiers:
                stages.add(error.summary["stage"])
                self.assertIs(error.after.header, D.header)
                self.assertIs(error.after.payload, D.payload)
                self.assertEqual(
                    (error.after.cursor.lower, error.after.cursor.upper), error.summary["cursor"]
                )
                self.assertEqual(error.saved.extent, 96)
                self.assertTrue(error.origin and error.trace)
                if error.summary["stage"] == "View short string":
                    self.assertIs(error.after, error.before)
                self.assertFalse(error.summary["unfinished_element_not_published"] is False)
            old = state.error_frontiers
            object.__setattr__(state, "error_frontiers", old[:-1])
            try:
                with self.assertRaisesRegex(AssertionError, "error.*changed"):
                    state._checked_errors()
            finally:
                object.__setattr__(state, "error_frontiers", old)
            item = frontiers[-1]
            value = item.origin
            object.__setattr__(item, "origin", frontiers[0].origin)
            try:
                with self.assertRaisesRegex(AssertionError, "error.*changed"):
                    state._checked_errors()
            finally:
                object.__setattr__(item, "origin", value)
        self.assertTrue({"Views length", "View id", "View short string", "View UTF8"} <= stages)


class TailEarlierErrorReturn(unittest.TestCase):
    def test_actual_Source_and_Tail_errors_keep_whole_Warning_carrier_and_original_returns(self):
        from config_decoded_tail import (
            _first_boot_tail_phases,
            _source_error_returns,
            _tail_error_returns,
        )
        from config_tail_uses import _tail_access_group

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        phases = _first_boot_tail_phases(objects, rows, source)
        bridge, uses = _tail_access_group(phases)
        errors = _source_error_returns(phases, bridge.control.structural) + _tail_error_returns(
            phases, bridge
        )
        source = phases.predecessor.phases
        expected = sum(
            len(p.array["errors"]) + (0 if p.remaining is None else len(p.remaining["errors"]))
            for p in source.phases
        )
        expected += sum(len(g["lineage"]["errors"]) for g in bridge.control.guards)
        self.assertEqual(len(errors), expected)
        self.assertTrue(errors)
        self.assertTrue(any(v.carrier >> 24 == 2 for v in errors))
        self.assertEqual({v.group for v in errors}, {"Sources", "suffix", "Tail"})
        for error in errors:
            self.assertEqual(error.carrier & 255, error.error)
            self.assertEqual(error.original_return[-1], 0x1001FE64 + 130)
            self.assertEqual(error.decoder.header.offset, 236)
            if error.carrier >> 24:
                self.assertEqual(error.carrier, 0x02000000 | error.error)

    def test_all_authored_Widget_carriers_consume_source_choices_not_diagnostic_labels(self):
        from config_decoded_tail import (
            _first_boot_tail_phases,
            _widget_error_transport,
            _earlier_error_returns,
        )
        from config_tail_control import _tail_structural
        from config_decoded_widgets import _widget_source_prefix, _advanced_source
        from config_abstract import Span

        objects, rows, source, _ = TailLiteralBodies.fixture(85)
        phases = _first_boot_tail_phases(objects, rows, source)
        raw = _tail_structural(phases)
        widgets = phases.predecessor.phases.widgets
        views = widgets.views
        before = views.exits[0].sequence.decoder
        # Separately labelled complete abstract adapter coverage, not a public
        # predecessor or a fabricated additional reachable Widget iteration.
        broad = replace(before, cursor=Span(before.cursor.lower, before.payload.extent))
        prefix = _widget_source_prefix(
            phases.objects, phases.rows, views, widgets.parents, widgets.children, broad
        )
        advanced = _advanced_source(
            phases.objects, phases.rows, views, widgets.parents, widgets.children, broad
        )
        for error in prefix.errors + advanced["errors"]:
            path, relation = _widget_error_transport(phases, raw, error["_origin"])
            self.assertEqual(path[-1], 0x1001C654 + 56)
            self.assertTrue(relation)
            if error["stage"].startswith("GraphRange::Fixed"):
                # Private conditional-record injection only. The complete
                # retained state graph rejects this mutation earlier.
                forged = replace(error["_origin"], trace=(("unrelated Fixed label",),))
                with self.assertRaisesRegex(AssertionError, "Fixed.*actual selector"):
                    _widget_error_transport(phases, raw, forged)
        actual = _earlier_error_returns(phases, raw)
        self.assertTrue(any(v.group == "Widgets" for v in actual))
        self.assertEqual(
            len(actual),
            len(views.context.error_frontiers) + len(views.error_frontiers) + len(widgets.errors),
        )

    def test_earlier_error_only_paths_reach_original_return_without_Tail_entries(self):
        from config_decoded_tail import _first_boot_tail_phases, _earlier_error_returns
        from config_tail_control import _tail_structural
        from unittest.mock import patch

        groups = set()
        for extent in (1, 4):
            objects, rows, source, _ = TailLiteralBodies.fixture(extent)
            with patch(
                "config_tail_source._controls_source",
                side_effect=AssertionError("fabricated Tail call"),
            ):
                phases = _first_boot_tail_phases(objects, rows, source)
            self.assertFalse(phases.entries)
            raw = _tail_structural(phases)
            errors = _earlier_error_returns(phases, raw)
            widgets = phases.predecessor.phases.widgets
            self.assertEqual(
                len(errors),
                len(widgets.views.context.error_frontiers)
                + len(widgets.views.error_frontiers)
                + len(widgets.errors),
            )
            self.assertTrue(errors)
            for error in errors:
                groups.add(error.group)
                self.assertEqual(error.carrier & 255, error.error)
                self.assertEqual(error.original_return[-1], 0x1001FE64 + 130)
                self.assertEqual(error.decoder.header.offset, 236)
                self.assertEqual(error.return_SP, widgets.views.context.frame.start)
                self.assertTrue(error.transport)
        self.assertEqual(groups, {"prefix", "Views", "Widgets"})


class WidgetPrivateErrorOrigins(unittest.TestCase):
    def test_actual_collection_keeps_length_and_loop_predecessors_and_detects_retained_drift(self):
        from config_decoded_widgets import _first_boot_decoded_widgets_state, _checked_widget_origin

        for extent in (1, 4, 85):
            objects, rows, source, _ = TailLiteralBodies.fixture(extent)
            public, state = _first_boot_decoded_widgets_state(objects, rows, source)
            frontiers = state._checked_errors()
            self.assertEqual(len(frontiers), len(public["group_errors"]))
            if extent == 1:
                self.assertEqual(frontiers, ())
                self.assertEqual(state.family_bridge.entries, ())
                self.assertTrue(state.views.context.error_frontiers)
                continue
            self.assertTrue(frontiers)
            for frontier in frontiers:
                origin = _checked_widget_origin(frontier.summary["_origin"], state.views.context)
                self.assertEqual(origin.after.cursor, frontier.summary["cursor"])
                if frontier.head is not None:
                    self.assertTrue(any(frontier.head is item for item in state.exits))
                    self.assertEqual(frontier.index, len(frontier.prefix))
            original = state.errors
            object.__setattr__(state, "errors", original[:-1])
            try:
                with self.assertRaisesRegex(AssertionError, "Widget private error"):
                    state._checked_errors()
            finally:
                object.__setattr__(state, "errors", original)
            frontier = frontiers[0]
            summary = frontier.summary
            origin = summary["_origin"]
            summary["_origin"] = replace(origin, error=99)
            try:
                with self.assertRaisesRegex(AssertionError, "Widget private error"):
                    state._checked_errors()
            finally:
                summary["_origin"] = origin
            self.assertEqual(state._checked_errors(), frontiers)

    def test_source_prefix_keeps_actual_call_choices_Fixed_guard_and_nested_Color_position(self):
        from config_decoded_widgets import _widget_source_prefix, _checked_widget_origin
        from test_config_stack import WidgetFiniteReturns
        from config_abstract import Span
        from dataclasses import replace

        objects, rows, state, parents, children = WidgetFiniteReturns.fixture()
        before = state.exits[0].sequence.decoder
        # Explicit abstract adapter input: the broad upper cursor also allows
        # short Fixed-float alternatives; the original narrow public head does not.
        result = _widget_source_prefix(
            objects,
            rows,
            state,
            parents,
            children,
            replace(before, cursor=Span(before.cursor.lower, before.payload.extent)),
        )
        nested = set()
        fixed = []
        for error in result.errors:
            origin = _checked_widget_origin(error["_origin"], state.context)
            self.assertEqual(origin.error, error["error"])
            self.assertEqual(origin.after.cursor, error["cursor"])
            self.assertEqual(origin.saved.extent, 96)
            if error["stage"].startswith("GraphRange::Fixed"):
                fixed.append(origin)
                self.assertIn("Fixed", repr(origin.trace))
            if error["stage"] == "WidgetColors.warning":
                inner = origin.call.choices[origin.choice_index]["nested_origin"]
                nested.add(inner[0])
                self.assertEqual(len(inner[4]), inner[0])
                if inner[0]:
                    self.assertIs(inner[4][-1][2]["state"], inner[1])
                    self.assertIs(inner[4][0][0].decoder, origin.call.before.decoder)
        self.assertTrue(fixed)
        self.assertEqual(nested, {0, 1})
        self.assertEqual({e["error"] for e in result.errors}, {4, 5, 6, 8, 14})

    def test_Band_and_Advanced_errors_keep_attempted_index_and_actual_before_outcome(self):
        from config_decoded_widgets import _advanced_source, _checked_widget_origin
        from test_config_stack import WidgetFiniteReturns
        from config_abstract import Span
        from dataclasses import replace

        objects, rows, state, parents, children = WidgetFiniteReturns.fixture()
        before = state.exits[0].sequence.decoder
        result = _advanced_source(
            objects,
            rows,
            state,
            parents,
            children,
            replace(before, cursor=Span(before.cursor.lower, before.payload.extent)),
        )
        stages = set()
        for error in result["errors"]:
            origin = _checked_widget_origin(error["_origin"], state.context)
            stages.add(error["stage"])
            self.assertEqual((origin.error, origin.after.cursor), (error["error"], error["cursor"]))
            if error["stage"].endswith(".label UTF8"):
                self.assertEqual(
                    origin.call[0], state.context.headers["visitor"]["proof"]["function"] + 2828
                )
            if error["stage"].startswith("Band3.") or error["stage"] == "Band capacity":
                self.assertIn("N>3", repr(origin.trace) + str(origin.condition))
        self.assertTrue(
            {
                "Bands length",
                "Band0.label short input",
                "Band3.label UTF8",
                "Band capacity",
                "Advanced.readout invalid enum",
            }
            <= stages
        )


class TailActivationPhysical(unittest.TestCase):
    @staticmethod
    def source(kind):
        from config_decoded_sources import _first_boot_decoded_sources_state
        from test_config_stack import SameBLoaderBoundary

        objects, rows, source, _ = TailLiteralBodies.fixture(72)
        elf = objects.elf
        S = 0x20080000
        V = S - 544 - 1608
        low = V - 152
        address = low - 4 if kind == "adjacent" else low
        if kind in ("section", "adjacent"):
            elf.sections.append((0, 8, 3, address, 0, 4, 0, 0, 4, 0))
        if kind in ("LOAD", "adjacent"):
            elf.segments.append((1, 0, address, address, 0, 4, 6, 4))
        if kind in ("OBJECT", "adjacent"):
            index = next(i for i, s in enumerate(elf.sections) if s[2] & 2)
            elf.symbol_entries.append(("supplied_Tail_lower_object", address, 4, 17, index))
        if kind == "bottom":
            old = elf.named[".uninit"]
            index = elf.sections.index(old)
            limit = V - 144
            new = tuple(limit - old[3] if i == 5 else v for i, v in enumerate(old))
            elf.sections[index] = new
            elf.named[".uninit"] = new
            elf.symbol_entries = [
                (n, limit if n in ("_stack_end", "__euninit") else a, z, t, i)
                for n, a, z, t, i in elf.symbol_entries
            ]
            elf.segments = [
                tuple(new[5] if i == 5 else v for i, v in enumerate(s)) if s[2] == old[3] else s
                for s in elf.segments
            ]
        _, state = _first_boot_decoded_sources_state(
            objects, rows, SameBLoaderBoundary.rebound(objects, source)
        )
        c = state.phases.widgets.views.context
        assert c.headers["visitor_frame"].start == V and state.exits
        return state, low

    def test_actual_entered_Tail_rejects_new_lower_eight_allocation_and_Reset_bottom(self):
        from config_decoded_tail import _tail_entries

        for kind in ("section", "LOAD", "OBJECT", "bottom"):
            state, low = self.source(kind)
            with (
                self.subTest(kind=kind),
                self.assertRaisesRegex(
                    AssertionError, "physical frame intersects|Reset-derived stack allocation"
                ),
            ):
                _tail_entries(state.phases.objects, state.phases.rows, state)

    def test_actual_entered_Tail_accepts_allocations_ending_at_exact_new_low_boundary(self):
        from config_decoded_tail import _tail_entries

        state, low = self.source("adjacent")
        entries = _tail_entries(state.phases.objects, state.phases.rows, state)
        self.assertTrue(entries)
        self.assertEqual(
            low, state.phases.widgets.views.context.headers["visitor_frame"].start - 152
        )

    def test_unentered_earlier_error_does_not_fabricate_the_deeper_Tail_activation(self):
        from config_decoded_tail import first_boot_decoded_visitor_return
        from test_config_stack import SameBLoaderBoundary

        objects, rows, source, _ = TailLiteralBodies.fixture(1)
        low = 0x20080000 - 544 - 1608 - 152
        objects.elf.sections.append((0, 8, 3, low, 0, 4, 0, 0, 4, 0))
        result = first_boot_decoded_visitor_return(
            objects, rows, SameBLoaderBoundary.rebound(objects, source)
        )
        self.assertEqual(result["Visitor_return"]["normal_returns"], 0)
        self.assertGreater(result["Visitor_return"]["error_returns"], 0)
        self.assertFalse(result["source_guarded_accesses"]["role_counts"])


class DetachedTypedNames(unittest.TestCase):
    def test_repeated_exact_detached_role_lookup_canonicalizes_each_name_only_once(self):
        from unittest.mock import patch
        import config_widget_children as child
        from config_structural import _detached_inputs
        from config_tail_contracts import ROLES

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        with patch.object(child, "canonical_symbol", wraps=child.canonical_symbol) as canonical:
            first = child._name(objects, ROLES["ControlAction"])
            self.assertEqual(child._name(objects, ROLES["ControlAction"], 0x1001BC64), first)
        self.assertEqual(canonical.call_count, len(objects.elf.functions))

    def test_complete_order_address_ambiguity_and_new_package_metadata_match_reference(self):
        from config_structural import _detached_inputs
        from config_stack import canonical_symbol
        from config_widget_children import _name
        from config_tail_contracts import ROLES

        original, rows, _, _ = TailLiteralBodies.fixture()
        frozen, detached = _detached_inputs(original, rows)
        reference = lambda objects, role, address=None: [
            n
            for n, e in objects.elf.functions.items()
            if canonical_symbol(n) == role and (address is None or e[0] & ~1 == address)
        ]
        for role in ROLES.values():
            names = reference(frozen, role)
            if len(names) != 1:
                continue
            address = frozen.elf.functions[names[0]][0] & ~1
            self.assertEqual(_name(frozen, role), names[0])
            self.assertEqual(_name(frozen, role, address), names[0])
            with self.assertRaisesRegex(AssertionError, "unique current typed role"):
                _name(frozen, role, address + 2)
        name = _name(frozen, ROLES["ControlAction"])
        entry = original.elf.functions[name]
        original.elf.functions[name + ".7654"] = entry
        self.assertEqual(_name(frozen, ROLES["ControlAction"]), name)
        new, newrows = _detached_inputs(original, rows)
        with self.assertRaisesRegex(AssertionError, "unique current typed role"):
            _name(new, ROLES["ControlAction"])
        self.assertEqual(reference(new, ROLES["ControlAction"]), [name, name + ".7654"])
        del original.elf.functions[name + ".7654"]
        original.elf.functions[name] = (entry[0] + 0x1000, *entry[1:])
        moved, _ = _detached_inputs(original, rows)
        # A different package's index is never a metadata-validation token.
        object.__setattr__(moved, "_typed_names", frozen._typed_names)
        self.assertEqual(_name(moved, ROLES["ControlAction"], (entry[0] & ~1) + 0x1000), name)
        self.assertIs(moved._typed_names[0], moved.elf.functions)
        with self.assertRaises(TypeError):
            moved._typed_names[1][ROLES["ControlAction"]] = ()
        self.assertIsInstance(moved._typed_names[1][ROLES["ControlAction"]], tuple)
        with self.assertRaisesRegex(AssertionError, "unique current typed role"):
            _name(frozen, ROLES["ControlAction"], (entry[0] & ~1) + 0x1000)
        with self.assertRaisesRegex(AssertionError, "unique current typed role"):
            _name(frozen, "unknown role")

    def test_unsupported_mutable_provider_retains_fresh_lookup_and_malformed_fallback(self):
        from config_widget_children import _name
        from config_structural import _FrozenObjects, _FrozenElf
        from config_tail_contracts import ROLES

        objects, rows, _, _ = TailLiteralBodies.fixture()
        name = _name(objects, ROLES["ControlAction"])
        entry = objects.elf.functions[name]
        objects.elf.functions[name + ".7654"] = entry
        with self.assertRaisesRegex(AssertionError, "unique current typed role"):
            _name(objects, ROLES["ControlAction"])
        del objects.elf.functions[name + ".7654"]
        self.assertEqual(_name(objects, ROLES["ControlAction"]), name)
        # Direct private construction is not a typed-metadata validation grant.
        malformed = _FrozenObjects(_FrozenElf(dict(functions={name: (entry[0],)})), {}, {})
        self.assertEqual(_name(malformed, ROLES["ControlAction"]), name)

    def test_complete_conditional_records_and_aliases_equal_legacy_lookup_with_fresh_dynamic_checks(
        self,
    ):
        from unittest.mock import patch
        from config_decoded_sources import _phase_graph
        from config_tail_source import _controls_source
        from config_tail_children import tail_coverage
        from config_tail_contracts import ROLES
        from config_widget_children import _name

        objects, rows, parents, state, frame, guards = TailControlsSource.fixture()
        with patch("config_widget_children._typed_names", return_value=None):
            _controls_source(
                objects, rows, parents, state, frame, guards
            )  # warm the same structural APIs
            old = _controls_source(objects, rows, parents, state, frame, guards)
        new = _controls_source(objects, rows, parents, state, frame, guards)
        self.assertEqual(_phase_graph(old), _phase_graph(new))
        # Warm names never authorize a stale body or a caller-selected role.
        proof = tail_coverage(objects, _name(objects, ROLES["ControlAction"]), rows)
        proof["action_targets"] = ()
        from config_tail_effects import control_action_interface

        args = list(TailActionInterface.fixture())
        args[2] = proof
        with self.assertRaisesRegex(AssertionError, "stale complete body"):
            control_action_interface(*args)


class TailPublicCurrentArtifacts(unittest.TestCase):
    def test_same_public_input_rejects_rebound_late_store_and_action_table_changes(self):
        import copy
        from config_decoded_tail import first_boot_decoded_visitor_return
        from test_config_stack import SameBLoaderBoundary, StartupConfigurationCopy
        from llvm_stack import Instruction

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        positive = first_boot_decoded_visitor_return(objects, rows, source)
        self.assertTrue(positive["successes"])
        for label, at, raw, operation, wanted in (
            (
                "Controls last hold",
                0x1001C654 + 6114,
                bytes.fromhex("aff30080"),
                "nop.w",
                "^foreign Views shared-error predecessor differs$",
            ),
            ("ControlAction table", 0x1001BC64 + 52, b"\x0b", None, "payload|table"),
        ):
            changed = copy.deepcopy(objects)
            records = dict(rows)
            StartupConfigurationCopy.put(changed, at, raw)
            if operation:
                records[at] = Instruction(at, raw, operation, "")
            current = SameBLoaderBoundary.rebound(changed, source)
            with self.subTest(label=label), self.assertRaisesRegex(AssertionError, wanted):
                first_boot_decoded_visitor_return(changed, records, current)

    def test_actual_public_Visitor_and_action_relocation_keeps_original_loader_return(self):
        from config_decoded_tail import first_boot_decoded_visitor_return
        from config_headers import VISITOR
        from config_tail_contracts import ROLES
        from test_config_stack import ActualDecodedViews, SameBLoaderBoundary

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        ActualDecodedViews.move_function(objects, rows, ROLES["ControlAction"], 0x10071000)
        ActualDecodedViews.move_function(objects, rows, VISITOR, 0x10078000)
        result = first_boot_decoded_visitor_return(
            objects, rows, SameBLoaderBoundary.rebound(objects, source)
        )
        self.assertTrue(result["successes"])
        self.assertEqual(result["physical_completion_sites"]["Controls"], 0x10078000 + 6114)
        self.assertEqual(result["Visitor_return"]["frontier"], 0x1001FE64 + 130)
        self.assertTrue(result["Visitor_return"]["original_saved_PC"])
        self.assertFalse(result["Visitor_return"]["loader_successor_executed"])


class DetachedInstructionRows(unittest.TestCase):
    def test_empty_and_custom_range_operands_keep_lazy_eager_fallback(self):
        from config_structural import _detached_inputs, _instruction_items
        from config_finite_returns import _body_selector
        from llvm_stack import Instruction

        objects, _, _, _ = TailLiteralBodies.fixture()
        frozen, empty = _detached_inputs(objects, {})
        self.assertEqual(_body_selector(empty, frozen)(None, 1), {})
        self.assertEqual(list(_instruction_items(frozen, empty, None, 1)), [])

        class Start(int):
            additions = 0

            def __add__(self, other):
                self.additions += 1
                return int(self) + other

        start = Start(10)
        row = Instruction(10, b"\x00\xbf", "nop", "")
        raw = {10: row, 11: replace(row, address=11), 12: replace(row, address=12)}
        self.assertEqual(dict(_instruction_items(objects, raw, start, 2)), {10: row, 11: raw[11]})
        self.assertEqual(start.additions, 3)

    def test_literal_and_return_selection_validate_exact_detached_map_once(self):
        import sys
        from config_structural import _detached_inputs
        from config_tail_children import tail_body
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_finite_returns import _body_selector

        objects, rows, _, _ = TailLiteralBodies.fixture()
        objects, rows = _detached_inputs(objects, rows)
        name = _name(objects, ROLES["ControlAction"])
        scans = []

        def observe(frame, event, arg):
            if (
                event == "c_call"
                and getattr(arg, "__self__", None) is rows
                and getattr(arg, "__name__", None) == "items"
            ):
                scans.append(frame.f_code.co_name)

        previous = sys.getprofile()
        sys.setprofile(observe)
        try:
            first = tail_body(objects, name, rows)
            again = tail_body(objects, name, rows)
            one = _body_selector(rows, objects)(first["function"], first["extent"])
            two = _body_selector(rows, objects)(first["function"], first["extent"])
        finally:
            sys.setprofile(previous)
        self.assertEqual(first, again)
        self.assertEqual(one, two)
        self.assertEqual(
            len(scans),
            1,
            "same immutable input must not rescan the whole map per body or activation",
        )

    def test_index_keeps_all_odd_keys_fresh_selections_and_foreign_package_association(self):
        from config_structural import _detached_inputs, _instruction_items
        from llvm_stack import Instruction

        objects, _, _, _ = TailLiteralBodies.fixture()
        start = 0x1001BC64
        raw = {
            start + i: Instruction(start + i, b"\x00\xbf", "nop", "") for i in (8, 5, 2, 1, 0, 7)
        }
        frozen, rows = _detached_inputs(objects, raw)
        first = list(_instruction_items(frozen, rows, start, 6))
        self.assertEqual(first, sorted((a, r) for a, r in rows.items() if start <= a < start + 6))
        self.assertEqual([a - start for a, r in first], [0, 1, 2, 5])
        first.clear()
        self.assertEqual(len(list(_instruction_items(frozen, rows, start, 6))), 4)
        for begin, extent in (
            (start, 2**50),
            (start, -1),
            (float(start), 2),
            (start, True),
            (start, 0),
            (start + 6, 1),
        ):
            with self.subTest(begin=begin, extent=extent):
                self.assertEqual(
                    dict(_instruction_items(frozen, rows, begin, extent)),
                    {a: r for a, r in rows.items() if begin <= a < begin + extent},
                )
        raw[start + 3] = Instruction(start + 3, b"\x00\xbf", "nop", "")
        current, new_rows = _detached_inputs(objects, raw)
        object.__setattr__(current, "_instruction_index", frozen._instruction_index)
        self.assertIn(start + 3, dict(_instruction_items(current, new_rows, start, 6)))
        self.assertIs(current._instruction_index[0], new_rows)
        self.assertIsInstance(current._instruction_index[1], tuple)
        self.assertNotIn(start + 3, dict(_instruction_items(frozen, rows, start, 6)))

    def test_custom_duplicates_and_malformed_private_construction_keep_eager_behavior(self):
        from config_structural import _instruction_items, _FrozenObjects, _detached_inputs
        from config_finite_returns import _body_selector
        from llvm_stack import Instruction

        objects, _, _, _ = TailLiteralBodies.fixture()
        at = 0x1001BC64
        first = Instruction(at, b"\x00\xbf", "nop", "")
        last = replace(first, operands="last")

        class ItemsOnly:
            def items(self):
                return ((at, first), (at, last), (at + 1, first))

            def __getitem__(self, key):
                raise AssertionError("unsupported provider indexing")

        custom = ItemsOnly()
        self.assertEqual(
            list(_instruction_items(objects, custom, at, 1)), [(at, first), (at, last)]
        )
        self.assertEqual(_body_selector(custom, objects)(at, 1), {at: last})
        frozen, _ = _detached_inputs(objects, {at: first})
        for values in ({at + 1: first}, {True: replace(first, address=1)}):
            malformed = _FrozenObjects(frozen.elf, {}, values)
            self.assertEqual(
                list(_instruction_items(malformed, malformed._rows, 0, 2**32)),
                list(malformed._rows.items()),
            )
            self.assertIsNone(malformed._instruction_index)
        malformed = _FrozenObjects(frozen.elf, {}, {str(at): first})
        with self.assertRaises(TypeError):
            list(_instruction_items(malformed, malformed._rows, at, 2))

    def test_complete_conditional_graph_matches_eager_selection_and_warm_inputs_stay_current(self):
        from unittest.mock import patch
        from config_decoded_sources import _phase_graph
        from config_tail_source import _controls_source
        from config_tail_children import tail_body
        from config_tail_contracts import ROLES
        from config_widget_children import _name
        from config_structural import _detached_inputs
        from test_config_stack import StartupConfigurationCopy

        objects, rows, parents, state, frame, guards = TailControlsSource.fixture()
        with patch("config_structural._instruction_addresses", return_value=None):
            _controls_source(objects, rows, parents, state, frame, guards)
            old = _controls_source(objects, rows, parents, state, frame, guards)
        new = _controls_source(objects, rows, parents, state, frame, guards)
        self.assertEqual(_phase_graph(old), _phase_graph(new))
        self.assertIsNot(old, new)
        original, raw, _, _ = TailLiteralBodies.fixture()
        frozen, current = _detached_inputs(original, raw)
        name = _name(frozen, ROLES["ControlAction"])
        before = tail_body(frozen, name, current)
        at = 0x1001BC64 + 44
        row = raw[at]
        changed = bytes((row.raw[0] ^ 1,)) + row.raw[1:]
        StartupConfigurationCopy.put(original, at, changed)
        raw[at] = replace(row, raw=changed, operands="r0, #0x13")
        self.assertEqual(tail_body(frozen, name, current), before)
        fresh, next_rows = _detached_inputs(original, raw)
        with self.assertRaisesRegex(AssertionError, "payload"):
            tail_body(fresh, name, next_rows)


class TailClosedProducer(unittest.TestCase):
    def test_actual_raw_producer_checks_the_full_incoming_entry_once_before_closed_consumption(
        self,
    ):
        from unittest.mock import patch
        import config_decoded_tail as subject

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        with patch.object(subject, "_tail_entries", wraps=subject._tail_entries) as entries:
            result, state = subject._first_boot_decoded_visitor_return_state(objects, rows, source)
        self.assertTrue(result["successes"])
        self.assertTrue(state.access_bridge.uses)
        self.assertTrue(state.error_exits)
        self.assertEqual(
            entries.call_count,
            1,
            "one closed newly-owned raw producer must not replay the full upstream entry",
        )


class TailClosedProducerRelations(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from unittest.mock import patch
        import config_decoded_tail as subject

        cls.inputs = TailLiteralBodies.fixture(128)[:3]
        with patch.object(subject, "_tail_entries", wraps=subject._tail_entries) as entries:
            cls.result, cls.state = subject._first_boot_decoded_visitor_return_state(*cls.inputs)
        cls.entry_checks = entries.call_count

    def test_complete_supplied_frozen_fallback_matches_fresh_raw_values_types_and_aliases(self):
        from unittest.mock import patch
        import config_decoded_tail as subject
        from config_structural import _detached_inputs

        objects, rows, source = self.inputs
        self.assertEqual(self.entry_checks, 1)
        supplied, current = _detached_inputs(objects, rows)
        self.assertFalse(subject._raw_composition_inputs(supplied, current, source))
        with patch.object(subject, "_tail_entries", wraps=subject._tail_entries) as entries:
            result, state = subject._first_boot_decoded_visitor_return_state(
                supplied, current, source
            )
        self.assertEqual(entries.call_count, 2)
        self.assertEqual(result, self.result)
        # These are the complete typed value/container-alias encodings, not
        # public envelope equality or translated evaluated cursor histories.
        self.assertEqual(state.phases.checkpoint, self.state.phases.checkpoint)
        self.assertEqual(
            state.access_bridge.control.checkpoint, self.state.access_bridge.control.checkpoint
        )
        self.assertEqual(state.access_bridge.checkpoint, self.state.access_bridge.checkpoint)
        self.assertIsNot(state.phases.objects, self.state.phases.objects)
        self.assertIsNot(state.phases.source_context, self.state.phases.source_context)
        self.assertGreater(len(state.phases.entries), 1)
        self.assertTrue(state.exits)
        self.assertTrue(state.error_exits)
        for value in (state, self.state):
            self.assertTrue(
                all(
                    u.entry is value.phases.entries[u.phase_index] for u in value.access_bridge.uses
                )
            )
            self.assertTrue(
                all(p.entry.predecessor is value.phases.predecessor for p in value.phases.phases)
            )
            self.assertTrue(
                all(
                    e.source_normal is value.phases.phases[e.phase_index].suffix["normal"]
                    for e in value.exits
                )
            )

    def test_complete_local_handoff_check_rejects_mutation_before_any_control_consumption(self):
        from unittest.mock import patch
        import config_decoded_tail as subject

        phases = self.state.phases
        phase = next(p for p in phases.phases if p.control_effects["complete_after"] is not None)
        original = phase.control_effects["complete_after"]
        phase.control_effects["complete_after"] = original - 2
        try:
            # A private handoff fault injected into an already constructed
            # fixture isolates the local comparison; it is not a supplied
            # public initializer or a mocked successful source validation.
            with (
                patch.object(subject, "_first_boot_tail_phases", return_value=phases),
                patch(
                    "config_tail_control._tail_control_data",
                    side_effect=AssertionError("consumed changed handoff"),
                ),
            ):
                with self.assertRaisesRegex(
                    AssertionError, "Tail private source/epoch/child/effect record changed"
                ):
                    subject._first_boot_decoded_visitor_return_state(*self.inputs)
        finally:
            phase.control_effects["complete_after"] = original

    def test_new_path_retained_aliases_members_and_late_effects_remain_checked(self):
        from config_tail_uses import _check_tail_uses

        state = self.state
        bridge = state.access_bridge
        control = bridge.control
        selection = control.structural.selection
        replacements = dict(
            context=replace(selection.context),
            source_selection=replace(selection.source_selection),
            widgets=replace(selection.widgets),
            Source_parents=tuple(list(selection.Source_parents)),
            Source_children=dict(selection.Source_children),
        )
        for field, value in replacements.items():
            original = getattr(selection, field)
            object.__setattr__(selection, field, value)
            try:
                with (
                    self.subTest(alias=field),
                    self.assertRaisesRegex(AssertionError, "association|structure|retained"),
                ):
                    control.current()
            finally:
                object.__setattr__(selection, field, original)
        object.__setattr__(control, "actual_reached_accesses_pending", False)
        try:
            with self.assertRaisesRegex(AssertionError, "retained reached-access qualification"):
                control.current()
        finally:
            object.__setattr__(control, "actual_reached_accesses_pending", True)
        for uses in (
            bridge.uses[:-1],
            tuple(reversed(bridge.uses)),
            bridge.uses + (bridge.uses[0],),
        ):
            with (
                self.subTest(members=len(uses)),
                self.assertRaisesRegex(AssertionError, "collection changed"),
            ):
                _check_tail_uses(replace(bridge, uses=uses))
        use = next(u for u in bridge.uses if u.role == "Screen_output_words")
        original = use.source["invocation"]
        use.source["invocation"] = dict(original)
        try:
            with self.assertRaisesRegex(AssertionError, "collection changed"):
                _check_tail_uses(bridge)
        finally:
            use.source["invocation"] = original
        phase = next(
            p
            for p in state.phases.phases
            if p.suffix_effects is not None and p.suffix_effects["complete_after"] is not None
        )
        original = phase.suffix_effects["complete_after"]
        phase.suffix_effects["complete_after"] = original - 2
        try:
            with self.assertRaisesRegex(AssertionError, "collection changed"):
                _check_tail_uses(bridge)
        finally:
            phase.suffix_effects["complete_after"] = original
        # A retained consumer still performs the complete fresh upstream
        # validation after all independently injected faults are restored.
        self.assertIs(state.current(), state)

    def test_raw_eligibility_is_not_a_caller_detached_or_custom_provider_grant(self):
        from config_decoded_tail import _raw_composition_inputs
        from config_stack import DiagnosticObjects
        from config_structural import _detached_inputs

        objects, rows, source = self.inputs
        self.assertTrue(_raw_composition_inputs(objects, rows, source))

        class Custom(DiagnosticObjects):
            pass

        self.assertFalse(_raw_composition_inputs(Custom(objects.elf, objects.sizes), rows, source))
        objects.read_only = lambda *args: (_ for _ in ()).throw(AssertionError("caller override"))
        try:
            self.assertFalse(_raw_composition_inputs(objects, rows, source))
        finally:
            del objects.read_only
        frozen, current = _detached_inputs(objects, rows)
        self.assertFalse(_raw_composition_inputs(frozen, current, source))
        self.assertFalse(_raw_composition_inputs(objects, dict(rows).items(), source))


class TailClosedProducerFallback(unittest.TestCase):
    def test_custom_reentry_and_fresh_non_site_reads_keep_independent_error_only_validation(self):
        from config_decoded_tail import (
            _first_boot_decoded_visitor_return_state,
            _raw_composition_inputs,
        )
        from config_stack import DiagnosticObjects

        objects, rows, source, _ = TailLiteralBodies.fixture(1)

        class Custom(DiagnosticObjects):
            changed = False
            entered = False
            inner = None

            def read_only(self, address, extent):
                if not self.entered:
                    self.entered = True
                    self.inner = _first_boot_decoded_visitor_return_state(objects, rows, source)
                raw = super().read_only(address, extent)
                if self.changed and address <= 0x1001BC64 + 44 < address + extent:
                    at = 0x1001BC64 + 44 - address
                    raw = raw[:at] + bytes((raw[at] ^ 1,)) + raw[at + 1 :]
                return raw

        custom = Custom(objects.elf, objects.sizes)
        self.assertFalse(_raw_composition_inputs(custom, rows, source))
        result, state = _first_boot_decoded_visitor_return_state(custom, rows, source)
        self.assertEqual(result, custom.inner[0])
        self.assertTrue(state.error_exits)
        self.assertFalse(state.phases.entries)
        self.assertIsNone(state.access_bridge)
        self.assertIs(state.phases.objects, custom)
        self.assertIsNot(state.phases.objects, custom.inner[1].phases.objects)
        custom.changed = True
        with self.assertRaisesRegex(AssertionError, "payload"):
            _first_boot_decoded_visitor_return_state(custom, rows, source)


class SourceTailClosedIntegration(unittest.TestCase):
    def test_combined_raw_path_keeps_first_Tail_Source_current_and_complete_return_relations(self):
        from unittest.mock import patch
        import config_decoded_sources as sources
        import config_decoded_tail as subject

        objects, rows, source, _ = TailLiteralBodies.fixture(128)
        current = sources._SourcesState.current
        checks = []

        def observed(state):
            checks.append(state)
            return current(state)

        with patch.object(sources._SourcesState, "current", observed):
            result, state = subject._first_boot_decoded_visitor_return_state(objects, rows, source)
        self.assertEqual(len(checks), 1)
        self.assertIs(checks[0], state.phases.predecessor)
        self.assertTrue(state.exits)
        self.assertTrue(state.error_exits)
        self.assertTrue(state.access_bridge.uses)
        self.assertTrue(
            all(u.entry is state.phases.entries[u.phase_index] for u in state.access_bridge.uses)
        )
        self.assertTrue(
            all(
                e.source_normal is state.phases.phases[e.phase_index].suffix["normal"]
                for e in state.exits
            )
        )
        self.assertEqual(len(state.error_exits), result["error_occurrences"])
        self.assertTrue(result["Visitor_return"]["original_saved_PC"])
        self.assertTrue(result["Visitor_return"]["Visitor_local_borrows_retired"])
        self.assertFalse(result["Visitor_return"]["loader_successor_executed"])
        self.assertTrue(
            all(e.return_SP == result["Visitor_return"]["return_SP"] for e in state.error_exits)
        )
        self.assertFalse(result["unconditional_owner"])
        self.assertFalse(result["exceptional_resume_established"])


if __name__ == "__main__":
    unittest.main()
