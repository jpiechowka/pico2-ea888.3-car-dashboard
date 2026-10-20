"""Fixture typing must preserve the production detachment path."""

import hashlib
from types import SimpleNamespace
import unittest
from typing import Literal, NotRequired, TypedDict

from config_stack import DiagnosticObjects
from config_structural import _FrozenObjects, _detached_inputs, _image_hash
from image_test_support import ImageFixture, checked_fixture, diagnostic_fixture, fixture_mapping
from llvm_stack import Instruction


class NestedFixture(TypedDict):
    count: int
    tag: Literal["entry"]


class RecordFixture(TypedDict):
    nested: NestedFixture
    enabled: bool
    coordinates: tuple[int, int]
    detail: NotRequired[str]


class FixtureProviders(unittest.TestCase):
    def test_mutation_view_keeps_backing_identity_and_rejects_non_dictionary(self) -> None:
        original = {4: 8}
        writable = fixture_mapping(original)
        self.assertIs(writable, original)
        writable["invalid-key"] = "invalid-value"
        self.assertIn("invalid-key", original)
        for invalid in ([(4, 8)], ((4, 8),), None):
            with self.subTest(invalid=invalid):
                with self.assertRaisesRegex(AssertionError, "plain dictionary"):
                    fixture_mapping(invalid)

    def test_exact_providers_detach_and_keep_invocation_cache_isolated(self) -> None:
        image = ImageFixture(data=b"original")
        sizes = {0x1000: 8}
        raw = diagnostic_fixture(image, sizes)
        rows: dict[int, Instruction] = {}
        self.assertIs(type(raw), DiagnosticObjects)
        self.assertIs(type(raw.elf), SimpleNamespace)
        self.assertIs(raw.elf.functions, image.functions)
        self.assertIs(raw.sizes, sizes)
        owned, detached_rows = _detached_inputs(raw, rows)
        self.assertIs(type(owned), _FrozenObjects)
        self.assertIsNot(owned, raw)
        self.assertIsNot(detached_rows, rows)
        if not isinstance(owned, _FrozenObjects):
            self.fail("plain diagnostic fixture must enter the detached path")
        expected = hashlib.sha256(b"original").hexdigest()
        self.assertEqual(_image_hash(owned), expected)
        self.assertEqual(owned._hashes, {"image": expected})
        raw.elf.data = b"mutated"
        sizes[0x1000] = 64
        self.assertEqual(_image_hash(owned), expected)
        self.assertEqual(owned.sizes[0x1000], 8)
        fresh, _ = _detached_inputs(raw, rows)
        self.assertIsNot(fresh, owned)
        self.assertEqual(_image_hash(fresh), hashlib.sha256(b"mutated").hexdigest())
        self.assertEqual(fresh.sizes[0x1000], 64)

    def test_custom_section_reader_data_is_not_silently_discarded(self) -> None:
        with self.assertRaisesRegex(AssertionError, "plain image metadata"):
            diagnostic_fixture(ImageFixture(section_data={".text": b"custom"}), {})

    def test_required_and_readonly_description_views_preserve_identity(self) -> None:
        from types import MappingProxyType
        from image_test_support import required_fixture, fixture_record, fixture_records

        child = MappingProxyType({"count": 3})
        records = (child,)
        self.assertIs(required_fixture(records), records)
        self.assertIs(fixture_record(child), child)
        self.assertIs(fixture_records(records), records)
        self.assertIs(fixture_records(records)[0], child)
        with self.assertRaises(AssertionError):
            required_fixture(None)
        invalid_cases: tuple[object, ...] = ({1: "wrong key"}, [], 4)
        for invalid in invalid_cases:
            with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                fixture_record(invalid)
        for invalid_records in ([child], (child, 3), ({False: 1},)):
            with self.subTest(invalid_records=invalid_records), self.assertRaises(AssertionError):
                fixture_records(invalid_records)

    def test_checked_schema_preserves_original_metadata_and_aliases(self) -> None:
        child = {"count": 3, "tag": "entry"}
        provenance = {"retained": ["original"]}
        raw = {
            "nested": child,
            "enabled": True,
            "coordinates": (4, 8),
            "provenance": provenance,
        }
        result = checked_fixture(raw, RecordFixture)
        self.assertIs(result, raw)
        self.assertIs(result["nested"], child)
        self.assertIs(raw["provenance"], provenance)
        self.assertNotIn("detail", result)

    def test_checked_schema_rejects_wrong_scalar_nested_and_tuple_fields(self) -> None:
        valid: dict[str, object] = {
            "nested": {"count": 3, "tag": "entry"},
            "enabled": True,
            "coordinates": (4, 8),
        }
        mutations = (
            {"nested": {"count": True, "tag": "entry"}},
            {"nested": {"count": 3}},
            {"nested": {"count": 3, "tag": "other"}},
            {"enabled": 1},
            {"coordinates": [4, 8]},
            {"coordinates": (4,)},
            {"detail": 9},
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaisesRegex(AssertionError, "RecordFixture"):
                    checked_fixture({**valid, **mutation}, RecordFixture)


if __name__ == "__main__":
    unittest.main()
