"""Supplied Tail access records; no actual decoder or graph completion."""

import copy
import json
import unittest
from dataclasses import replace
from config_platform import Dependency
from config_source_access import SourceFamily
from config_tail_access import TailFamily
from script_values import array_items, object_fields, integer_value
from image_test_support import fixture_fields, fixture_items, dynamic_call


def tail_family(value: Dependency) -> TailFamily:
    if not isinstance(value, TailFamily):
        raise AssertionError("Tail fixture contains another family")
    return value


class TailPolicyV8(unittest.TestCase):
    @staticmethod
    def augment(values: tuple[Dependency, ...]) -> tuple[Dependency, ...]:
        # Supplied dependency geometry only; no decoded source or completed
        # ordinary path is manufactured by this test transport.
        from config_regions import Storage, borrow
        from config_tail_access import TailAccessContext, TailFamily

        if len(values) != 24:
            raise AssertionError("Tail fixture requires all 24 prior dependencies")
        source = values[23]
        if not isinstance(source, SourceFamily):
            raise AssertionError("Tail fixture requires its Source predecessor")
        old = source.context
        phase = "decoded_controls_tail_group"
        loader = replace(old.loader, phase=phase)
        visitor = replace(old.visitor, phase=phase)
        config = replace(old.config.storage, phase=phase)
        child = Storage(
            visitor.space, "supplied ScreenBindings", visitor.start - 64, 64, 8, phase, True
        )
        guards = tuple(replace(v, phase=phase) for v in old.protected)
        shapes = (
            (
                "Screen_output_words",
                0x1001B900,
                (
                    (
                        296,
                        "c4f80600",
                        6,
                        4,
                        "write",
                        "same_Screen_fields_6_to_10_before_completion",
                    ),
                    (306, "c4f80200", 2, 4, "write", "same_Screen_fields_2_to_6_before_completion"),
                ),
            ),
            (
                "first_Screen_scratch_words",
                source.caller,
                (
                    (6496, "ddf8b203", 6, 4, "read", "first_completed_Screen_fields_6_to_10"),
                    (6510, "ddf8ae03", 2, 4, "read", "first_completed_Screen_fields_2_to_6"),
                ),
            ),
            (
                "first_Config_Screen_words",
                source.caller,
                (
                    (6074, "c6f8da20", 6, 4, "write", "first_Screen_staged_fields_6_to_10"),
                    (6078, "c6f8d600", 2, 4, "write", "first_Screen_staged_fields_2_to_6"),
                ),
            ),
        )
        added = []
        for index, (role, parent, sites) in enumerate(shapes):
            local = index == 0
            protected = guards + (
                (Storage(visitor.space, "Screen saves", child.start + 28, 36, 4, phase, False),)
                if local
                else ()
            )
            context = TailAccessContext(
                loader,
                visitor,
                child if local else visitor,
                borrow(config, 0, 14576, phase),
                protected,
            )
            epochs = (
                tuple(
                    (source.caller + call, out, count)
                    for call, out, count in ((5712, 940, 3), (6198, 1000, 2), (6426, 1064, 1))
                )
                if local
                else ()
            )
            added.append(
                TailFamily(
                    role,
                    "sizez",
                    parent,
                    source.caller,
                    tuple(
                        (parent + off, raw, offset, width, direction, epoch)
                        for off, raw, offset, width, direction, epoch in sites
                    ),
                    context,
                    epochs,
                )
            )
        return values + tuple(added)

    def test_prior_24_dependencies_cannot_replace_the_three_tail_requirements(self) -> None:
        from test_config_stack import ConditionalStackPolicy, ValidatorPlatformFamilies
        import config_platform as subject

        values = ValidatorPlatformFamilies().dependencies("sizez")[:24]
        self.assertEqual(len(values), 24)
        objects, view, context = ConditionalStackPolicy().fixture()
        with self.assertRaisesRegex(AssertionError, "Tail family"):
            subject.reviewed_eav_policy(
                objects, view, context, contradictory_effects=(), owned_dependencies=values
            )

    def test_previous_scope_is_rejected_even_with_the_same_input_and_no_owned_accesses(
        self,
    ) -> None:
        from test_config_stack import ConditionalStackPolicy
        import config_platform as subject

        objects, view, context = ConditionalStackPolicy().fixture()
        policy = subject.reviewed_eav_policy(objects, view, context, contradictory_effects=())
        record = policy.to_record()
        record["schema"] = 6
        record["scope"] = "h2-rp2350-secure-owned-access-v7"
        record["scope_sha256"] = "50408f1a4774caff701c236184a4f17060cb9e17ade5fffaa0ca208d4f1b6181"
        with self.assertRaises(AssertionError):
            subject.read_policy(record, policy)

    def test_complete_conditional_tuple_roundtrips_without_claiming_completion(self) -> None:
        from test_config_stack import ConditionalStackPolicy, ValidatorPlatformFamilies
        import config_platform as subject

        values = ValidatorPlatformFamilies().dependencies("sizez")
        objects, view, context = ConditionalStackPolicy().fixture()
        policy = subject.reviewed_eav_policy(
            objects, view, context, contradictory_effects=(), owned_dependencies=values
        )
        self.assertEqual(len(values), 27)
        self.assertEqual(
            (policy.schema, policy.scope, policy.scope_sha256),
            (
                7,
                "h2-rp2350-secure-owned-access-v8",
                "0decd7e28e1eaa62d95730efe4bb0eb02ced8f457c92939190c0f7eb722bcf03",
            ),
        )
        raw_record: object = json.loads(json.dumps(policy.to_record()))
        record = object_fields(raw_record, "policy roundtrip")
        self.assertEqual(subject.read_policy(record, policy), policy)
        self.assertFalse(record["hardware_qualified"])
        self.assertEqual(
            tuple(tail_family(v).role for v in values[24:]),
            ("Screen_output_words", "first_Screen_scratch_words", "first_Config_Screen_words"),
        )
        self.assertTrue(
            all(
                tail_family(v).to_record()["physical_tag_is_completed_Tail"] is False
                for v in values[24:]
            )
        )

    def test_missing_reordered_cross_caller_or_cross_activation_tail_is_rejected(self) -> None:
        from test_config_stack import ConditionalStackPolicy, ValidatorPlatformFamilies
        import config_platform as subject

        values = ValidatorPlatformFamilies().dependencies("sizez")
        objects, view, context = ConditionalStackPolicy().fixture()
        for fault in ("missing", "duplicate", "reorder", "caller", "loader", "visitor", "config"):
            changed = list(values)
            if fault == "missing":
                changed.pop()
            elif fault == "duplicate":
                changed[-1] = changed[-2]
            elif fault == "reorder":
                changed[-3], changed[-2] = changed[-2], changed[-3]
            else:
                family = changed[-3]
                if not isinstance(family, TailFamily):
                    raise AssertionError("Tail fixture lost its tail family")
                c = family.context
                if fault == "caller":
                    changed[-3] = replace(
                        family,
                        caller=family.caller + 0x1000,
                        epochs=tuple((at + 0x1000, out, n) for at, out, n in family.epochs),
                    )
                elif fault == "loader":
                    c = replace(c, loader=replace(c.loader, identity="another loader"))
                elif fault == "visitor":
                    c = replace(c, visitor=replace(c.visitor, identity="another Visitor"))
                else:
                    c = replace(
                        c,
                        config=replace(
                            c.config,
                            storage=replace(c.config.storage, start=c.config.start + 0x4000),
                        ),
                        protected=tuple(
                            replace(v, start=v.start + 0x4000) if v.space == "address" else v
                            for v in c.protected
                        ),
                    )
                if fault != "caller":
                    changed[-3] = replace(family, context=c)
            with self.subTest(fault=fault), self.assertRaises(AssertionError):
                subject.reviewed_eav_policy(
                    objects,
                    view,
                    context,
                    contradictory_effects=(),
                    owned_dependencies=tuple(changed),
                )


class TailAccessRecords(unittest.TestCase):
    @staticmethod
    def values() -> tuple[TailFamily, ...]:
        from test_config_stack import ValidatorPlatformFamilies

        values = ValidatorPlatformFamilies().dependencies("sizez")[24:]
        result = []
        for value in values:
            if not isinstance(value, TailFamily):
                raise AssertionError("Tail fixture contains another family")
            result.append(value)
        return tuple(result)

    def test_three_output_epochs_and_first_only_parent_footprints(self) -> None:
        from config_tail_access import read_tail_family

        a, b, c = self.values()
        start = a.context.visitor.start
        for index, offset in enumerate((940, 1000, 1064)):
            self.assertEqual(
                a.footprints(index),
                (
                    (start + offset + 6, start + offset + 10),
                    (start + offset + 2, start + offset + 6),
                ),
            )
        self.assertEqual(b.footprints(0), ((start + 946, start + 950), (start + 942, start + 946)))
        B = c.context.config.start
        self.assertEqual(c.footprints(0), ((B + 218, B + 222), (B + 214, B + 218)))
        for value in (a, b, c):
            self.assertEqual(read_tail_family(json.loads(json.dumps(value.to_record()))), value)
            for bad in (-1, True, 1.0, 3):
                with self.subTest(role=value.role, index=bad), self.assertRaises(AssertionError):
                    dynamic_call(value.footprints, bad)
        for value in (b, c):
            with self.assertRaises(AssertionError):
                value.footprints(1)

    def test_reader_rejects_wrong_width_role_epoch_numeric_type_and_claimed_completion(
        self,
    ) -> None:
        from config_tail_access import read_tail_family

        for family in self.values():
            original = family.to_record()
            for fault in (
                "missing",
                "extra",
                "mode",
                "role",
                "site",
                "float_width",
                "bool_offset",
                "direction",
                "source_phase",
                "complete",
                "frame",
                "protection",
                "config",
                "duplicate_site",
                "epoch",
            ):
                bad = copy.deepcopy(original)
                sites = fixture_items(bad["sites"])
                first_site = fixture_fields(sites[0])
                context = fixture_fields(bad["context"])
                frame = fixture_fields(context["frame"])
                config = fixture_fields(context["config"])
                epochs = fixture_items(bad["epochs"])
                if fault == "missing":
                    bad.pop("source_phase")
                elif fault == "extra":
                    bad["approved"] = True
                elif fault == "mode":
                    bad["mode"] = "sizes"
                elif fault == "role":
                    bad["role"] = "result_words"
                elif fault == "site":
                    first_site["site"] = integer_value(first_site["site"], "site") + 2
                elif fault == "float_width":
                    first_site["width"] = 4.0
                elif fault == "bool_offset":
                    first_site["offset"] = True
                elif fault == "direction":
                    first_site["direction"] = "execute"
                elif fault == "source_phase":
                    first_site["source_phase"] = "same_width_LED"
                elif fault == "complete":
                    bad["physical_tag_is_completed_Tail"] = True
                elif fault == "frame":
                    frame["extent"] = integer_value(frame["extent"], "extent") - 8
                elif fault == "protection":
                    fixture_items(context["protected"]).pop(0)
                elif fault == "config":
                    fixture_fields(config["storage"])["identity"] = "raw pointer"
                elif fault == "duplicate_site":
                    sites[1] = copy.deepcopy(sites[0])
                elif epochs:
                    fixture_items(epochs[1])[1] = 940
                else:
                    bad["epochs"] = [[integer_value(bad["caller"], "caller") + 5712, 940, 3]]
                with self.subTest(role=family.role, fault=fault), self.assertRaises(AssertionError):
                    read_tail_family(bad)

    def test_physical_alias_and_saved_cell_overlap_reject_despite_space_labels(self) -> None:
        from config_regions import Storage

        for family in self.values():
            c = family.context
            first, end = family.footprints(0)[0]
            guard = Storage(
                "address", "physical alias guard", first, end - first, 1, c.loader.phase, False
            )
            with (
                self.subTest(role=family.role),
                self.assertRaisesRegex(AssertionError, "protected"),
            ):
                replace(family, context=replace(c, protected=c.protected + (guard,)))

    def test_current_artifact_rechecks_six_sites_typed_roles_and_relocated_metadata(self) -> None:
        from config_tail_access import current_tail_family
        from test_config_stack import ValidatorPlatformFamilies

        for delta in (0, 0x2000):
            objects = ValidatorPlatformFamilies().artifact("sizez")
            if delta:
                section = objects.elf.sections[0]
                objects.elf.sections[0] = (
                    section[0],
                    section[1],
                    section[2],
                    section[3] + delta,
                    section[4],
                    section[5],
                    section[6],
                    section[7],
                    section[8],
                    section[9],
                )
                objects.elf.functions = {
                    n: (at + delta, extent, section)
                    for n, (at, extent, section) in objects.elf.functions.items()
                }
                objects.elf.mapping_symbols = [
                    (n, at + delta, section) for n, at, section in objects.elf.mapping_symbols
                ]
                objects.sizes = {at + delta: size for at, size in objects.sizes.items()}
            for family in self.values():
                current = replace(
                    family,
                    parent=family.parent + delta,
                    caller=family.caller + delta,
                    sites=tuple(
                        (at + delta, raw, off, width, direction, epoch)
                        for at, raw, off, width, direction, epoch in family.sites
                    ),
                    epochs=tuple((at + delta, out, count) for at, out, count in family.epochs),
                )
                self.assertEqual(current_tail_family(objects, current), current)
                if delta:
                    with self.assertRaises(AssertionError):
                        current_tail_family(objects, family)
                for at, *_ in current.sites:
                    old = objects.elf.data
                    bad = bytearray(old)
                    bad[at - objects.elf.sections[0][3]] ^= 1
                    objects.elf.data = bytes(bad)
                    try:
                        with (
                            self.subTest(role=family.role, delta=delta, site=at),
                            self.assertRaisesRegex(AssertionError, "Tail operation bytes"),
                        ):
                            current_tail_family(objects, current)
                    finally:
                        objects.elf.data = old

    def test_current_metadata_cannot_substitute_a_different_role_frame_or_mapping(self) -> None:
        from config_tail_access import current_tail_family
        from config_tail_contracts import ROLES
        from config_headers import VISITOR
        from test_config_stack import ValidatorPlatformFamilies

        for family in self.values():
            for fault in (
                "frame",
                "extent",
                "role",
                "caller_frame",
                "caller_role",
                "mapping",
                "config_extent",
            ):
                objects = ValidatorPlatformFamilies().artifact("sizez")
                name = ROLES["ScreenBindings"] if family.epochs else VISITOR
                if fault == "frame":
                    objects.sizes[family.parent] -= 8
                elif fault == "extent":
                    at, size, section = objects.elf.functions[name]
                    objects.elf.functions[name] = (at, size - 2, section)
                elif fault == "role":
                    objects.elf.functions[name + "_wrong_type"] = objects.elf.functions.pop(name)
                elif fault == "caller_frame":
                    objects.sizes[family.caller] -= 8
                elif fault == "caller_role":
                    objects.elf.functions[VISITOR + "_wrong_type"] = objects.elf.functions.pop(
                        VISITOR
                    )
                elif fault == "mapping":
                    objects.elf.mapping_symbols = [("$d", objects.elf.sections[0][3], 0)]
                else:
                    role, at, size, kind, section = objects.elf.symbol_entries[0]
                    objects.elf.symbol_entries = [(role, at, size - 4, kind, section)]
                with self.subTest(role=family.role, fault=fault), self.assertRaises(AssertionError):
                    current_tail_family(objects, family)

    def test_image_reader_rechecks_all_six_tail_sites_with_a_coherently_updated_digest(
        self,
    ) -> None:
        import hashlib
        from config_platform import current_image_policy
        from test_config_stack import ValidatorPlatformFamilies

        elf, eav, stack = ValidatorPlatformFamilies().image_fixture("sizez")
        record = stack["policies"][0]
        self.assertEqual(len(current_image_policy(record, elf, eav).owned_dependencies), 27)
        original = elf.data
        for dependency in record["owned_dependencies"][24:]:
            for raw_site in array_items(dependency["sites"], "tail sites"):
                site = object_fields(raw_site, "tail site")
                at = integer_value(site["site"], "site")
                section = next(s for s in elf.sections if s[3] <= at < at + 4 <= s[3] + s[5])
                changed = bytearray(original)
                changed[section[4] + at - section[3]] ^= 1
                elf.data = bytes(changed)
                policy = copy.deepcopy(record)
                policy["elf_sha256"] = hashlib.sha256(elf.data).hexdigest()
                try:
                    with (
                        self.subTest(role=dependency["role"], site=at),
                        self.assertRaisesRegex(AssertionError, "Tail operation bytes"),
                    ):
                        current_image_policy(policy, elf, eav)
                finally:
                    elf.data = original


if __name__ == "__main__":
    unittest.main()
