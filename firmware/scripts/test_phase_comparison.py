"""Comparison parity and allocation checks against the unchanged writer."""

import gc
import sys
import tracemalloc
import unittest
import weakref
from dataclasses import dataclass, make_dataclass
from types import MappingProxyType

import config_decoded_sources as subject


def matches(value, baseline):
    return subject._phase_matches(value, baseline)


def outcome(call):
    try:
        return ("return", call())
    except Exception as error:
        return ("raise", type(error), str(error))


class PhaseComparison(unittest.TestCase):
    def test_literals_aliases_order_types_and_node_counts(self):
        shared = [1]
        baseline = (
            ("reference", 0),
            (
                ("list", (("reference", 1), ("reference", 2), ("reference", 1))),
                ("list", (("value", "int", 1),)),
                ("list", (("value", "int", 1),)),
            ),
        )
        for value, wanted in (
            ([shared, [1], shared], True),
            ([shared, shared, shared], False),
            ([[1], [1], [1]], False),
            ([shared, [True], shared], False),
            ([shared, (1,), shared], False),
            ([shared, [1]], False),
            ([shared, [1], shared, []], False),
        ):
            with self.subTest(value=value):
                self.assertIs(matches(value, baseline), wanted)
                self.assertIs(subject._phase_graph(value) == baseline, wanted)
        for live, saved, wanted in (
            (True, 1, False),
            ("x", b"x", False),
            (None, None, True),
            (7, [], False),
            ({"a": 1, "b": 2}, {"b": 2, "a": 1}, False),
            (MappingProxyType({"a": 1}), {"a": 1}, True),
        ):
            baseline = subject._phase_graph(saved)
            self.assertIs(matches(live, baseline), wanted)

    def test_plain_dataclasses_preserve_fields_metadata_and_aliases(self):
        @dataclass
        class Record:
            value: object
            flag: bool = False

        value = Record([1])
        baseline = subject._phase_graph([value, value])
        self.assertTrue(matches([value, value], baseline))
        self.assertFalse(matches([value, Record([1])], baseline))
        value.flag = True
        self.assertFalse(matches([value, value], baseline))

    def test_malformed_baselines_preserve_equality_and_late_traversal_errors(self):
        for baseline in (
            None,
            (),
            ((),),
            ((), (), ()),
            [(), ()],
            (("reference", 0), []),
            (("reference", 0), (7,)),
            (("reference", 0), (("list", ()), "extra")),
            (None, (("list", ()),)),
        ):
            with self.subTest(baseline=baseline):
                self.assertFalse(matches([], baseline))
                self.assertFalse(subject._phase_graph([]) == baseline)
                self.assertEqual(
                    outcome(lambda: matches([object()], baseline)),
                    ("raise", AssertionError, "unsupported Sources phase snapshot value"),
                )

    def test_earlier_difference_does_not_hide_late_invalid_values(self):
        cycle = []
        cycle.append(cycle)
        baseline = subject._phase_graph([0, []])
        for invalid, message in (
            (cycle, "cyclic Sources phase snapshot"),
            (object(), "unsupported Sources phase snapshot value"),
        ):
            wanted = ("raise", AssertionError, message)
            self.assertEqual(outcome(lambda: matches([1, invalid], baseline)), wanted)
            self.assertEqual(
                outcome(lambda: subject._phase_graph([1, invalid]) == baseline), wanted
            )

    def check_trace(self, factory, wanted, trace):
        for compare in (lambda v, b: subject._phase_graph(v) == b, matches):
            live, baseline, events = factory()
            self.assertEqual(outcome(lambda: compare(live, baseline)), wanted)
            self.assertEqual(events, trace)

    def test_getters_finish_before_metadata_equality_and_hash_is_never_called(self):
        def fixture():
            events = []

            class Metadata:
                def __init__(self, label):
                    self.label = label

                def __hash__(self):
                    raise RuntimeError("metadata must not be hashed")

                def __eq__(self, other):
                    events.append("EQ:" + self.label + ":" + other.label)
                    return True

            @dataclass
            class Record:
                value: int

                def __getattribute__(self, name):
                    if name == "value":
                        events.append("GET:" + str(object.__getattribute__(self, name)))
                    return object.__getattribute__(self, name)

            Record.__module__ = Metadata("base")
            baseline = subject._phase_graph([Record(1), Record(2)])
            Record.__module__ = Metadata("live")
            events.clear()
            return [Record(1), Record(2)], baseline, events

        self.check_trace(
            fixture, ("return", True), ["GET:1", "GET:2", "EQ:live:base", "EQ:live:base"]
        )

    def test_late_getter_exception_precedes_an_earlier_mismatch(self):
        def fixture():
            events = []

            @dataclass
            class Record:
                value: int

                def __getattribute__(self, name):
                    if name == "value":
                        events.append("GET")
                        raise RuntimeError("late getter")
                    return object.__getattribute__(self, name)

            return [1, Record(1)], subject._phase_graph([0, []]), events

        self.check_trace(fixture, ("raise", RuntimeError, "late getter"), ["GET"])

    def test_earlier_canonical_mismatch_skips_metadata_equality_after_full_traversal(self):
        def fixture():
            events = []

            class Metadata:
                __hash__ = None

                def __eq__(self, other):
                    raise RuntimeError("unreached equality")

            @dataclass
            class Record:
                value: int

                def __getattribute__(self, name):
                    if name == "value":
                        events.append("GET")
                    return object.__getattribute__(self, name)

            Record.__module__ = Metadata()
            baseline = subject._phase_graph([0, Record(1)])
            Record.__module__ = Metadata()
            events.clear()
            return [1, Record(1)], baseline, events

        self.check_trace(fixture, ("return", False), ["GET"])

    def test_captured_unhashable_metadata_survives_getter_class_mutation(self):
        def fixture():
            events = []

            @dataclass
            class Record:
                value: int

                def __getattribute__(self, name):
                    if name == "value":
                        events.append("GET")
                        type(self).__module__ = "changed"
                    return object.__getattribute__(self, name)

            Record.__module__ = ["captured"]
            baseline = subject._phase_graph(Record(1))
            Record.__module__ = ["captured"]
            events.clear()
            return Record(1), baseline, events

        self.check_trace(fixture, ("return", True), ["GET"])

    def test_callback_equality_order_is_preorder_with_short_circuit_and_mutation(self):
        def fixture(stop):
            events = []

            class Metadata:
                __hash__ = None

                def __init__(self, name):
                    self.name, self.changed = name, False

                def __eq__(self, other):
                    events.append(self.name)
                    if self.name == "outer":
                        later.changed = True
                        if stop == "raise":
                            raise ValueError("outer equality")
                        return stop != "false"
                    return self.changed

            outer, later = Metadata("outer"), Metadata("inner")
            A = make_dataclass("Outer", [("value", object)])
            B = make_dataclass("Inner", [("value", object)])
            A.__module__, B.__module__ = Metadata("base outer"), Metadata("base inner")
            baseline = subject._phase_graph(A(B(1)))
            A.__module__, B.__module__ = outer, later
            return A(B(1)), baseline, events

        self.check_trace(lambda: fixture("true"), ("return", True), ["outer", "inner"])
        self.check_trace(lambda: fixture("false"), ("return", False), ["outer"])
        self.check_trace(
            lambda: fixture("raise"), ("raise", ValueError, "outer equality"), ["outer"]
        )

    def test_custom_metaclass_dispatch_and_scalar_hash_callbacks_use_writer_once(self):
        def fixture():
            events = []

            class Meta(type):
                def __eq__(cls, other):
                    events.append("TYPE:" + other.__name__)
                    return other is int

                __hash__ = type.__hash__

            class Scalar(int, metaclass=Meta):
                def __hash__(self):
                    events.append("HASH")
                    return int.__hash__(self)

            @dataclass
            class Record:
                value: object

                def __getattribute__(self, name):
                    if name == "value":
                        events.append("GET")
                    return object.__getattribute__(self, name)

            live = Record(Scalar(1))
            baseline = subject._phase_graph(live)
            events.clear()
            return live, baseline, events

        live, baseline, events = fixture()
        self.assertTrue(subject._phase_graph(live) == baseline)
        expected = list(events)
        self.assertEqual(expected[0:4], ["GET", "TYPE:str", "TYPE:bytes", "TYPE:int"])
        self.assertEqual(expected.count("GET"), 1)
        self.assertGreater(expected.count("HASH"), 0)
        self.check_trace(fixture, ("return", True), expected)

    def test_proxy_custom_mapping_is_not_a_preflight_callback(self):
        from collections.abc import Mapping

        def fixture():
            events = []

            class Custom(Mapping):
                def __getitem__(self, key):
                    events.append("GETITEM")
                    return 1

                def __iter__(self):
                    events.append("ITER")
                    return iter(("x",))

                def __len__(self):
                    events.append("LEN")
                    return 1

            live = MappingProxyType(Custom())
            return live, subject._phase_graph({"x": 1}), events

        self.check_trace(fixture, ("return", True), ["ITER", "GETITEM"])

    def test_property_default_custom_metaclass_and_instance_field_override_fall_back(self):
        def fixture(mode):
            events = []

            @dataclass
            class Record:
                value: int

            live = Record(1)
            if mode == "property":

                def getter(self):
                    events.append("PROPERTY")
                    return 1

                Record.value = property(getter)
            elif mode == "default":

                class Meta(type):
                    def __eq__(cls, other):
                        events.append("DEFAULT TYPE")
                        return False

                class Default(metaclass=Meta):
                    pass

                Record.value = Default()
            else:
                live.__dataclass_fields__ = {}
            baseline = subject._phase_graph(live)
            events.clear()
            return live, baseline, events

        for mode, trace in (("property", ["PROPERTY"]), ("default", []), ("override", [])):
            with self.subTest(mode=mode):
                self.check_trace(lambda: fixture(mode), ("return", True), trace)

    def test_inherited_class_descriptor_does_not_read_a_shadow_dictionary_value(self):
        @dataclass(init=False)
        class Record:
            __class__: object

        value = Record()
        value.__dict__["__class__"] = 1
        baseline = subject._phase_graph([0, 1])
        wanted = ("raise", AssertionError, "unsupported Sources phase snapshot value")
        self.assertEqual(outcome(lambda: subject._phase_graph([1, value]) == baseline), wanted)
        self.assertEqual(outcome(lambda: matches([1, value], baseline)), wanted)

    def test_cleanup_releases_preflight_and_comparison_tables_without_cyclic_gc(self):
        visitors = []
        tables = []
        counts = []

        def observe(frame, event, arg):
            if frame.f_code.co_filename != subject.__file__:
                return
            if frame.f_code.co_name not in ("plain", "plain_encoded", "visit"):
                return
            owner = frame.f_back
            while owner is not None and owner.f_code is not subject._phase_matches.__code__:
                owner = owner.f_back
            if owner is None:
                return
            if event == "call":
                visitor = owner.f_locals["visit"]
                if not any(ref() is visitor for ref in visitors):
                    visitors.append(weakref.ref(visitor))
                    tables.append(
                        tuple(
                            owner.f_locals[name] for name in ("seen", "classes", "memo", "active")
                        )
                    )
            if event == "return" and frame.f_code.co_name == "visit":
                counts.append(len(owner.f_locals["memo"]))

        enabled, previous = gc.isenabled(), sys.getprofile()
        gc.disable()
        try:
            value = self.substantial_graph()
            baseline = subject._phase_graph(value)
            sys.setprofile(observe)
            self.assertTrue(matches(value, baseline))
            value[-1].index = -1
            self.assertFalse(matches(value, baseline))
            cycle = []
            cycle.append(cycle)
            with self.assertRaisesRegex(AssertionError, "cyclic"):
                matches(cycle, baseline)
            with self.assertRaisesRegex(AssertionError, "unsupported"):
                matches(object(), baseline)
            sys.setprofile(previous)
            self.assertEqual(max(counts), 12002)
            self.assertEqual(len(visitors), 4)
            self.assertTrue(all(not table for group in tables for table in group))
            self.assertTrue(all(ref() is None for ref in visitors))
            self.assertEqual(value[-1].index, -1)
            self.assertEqual(baseline[1][-3][3][0], ("index", ("value", "int", 3999)))
        finally:
            sys.setprofile(previous)
            if enabled:
                gc.enable()

    def test_unchanged_writer_handles_unusual_metadata_numeric_equality(self):
        A = make_dataclass("Same", [("value", int)])
        B = make_dataclass("Same", [("value", int)])
        A.__module__, B.__module__ = True, 1
        self.assertTrue(matches(A(7), subject._phase_graph(B(7))))

    def test_actual_tail_controls_records_compare_without_a_second_snapshot(self):
        from config_tail_source import _controls_source
        from test_config_tail import TailControlsSource

        value = _controls_source(*TailControlsSource.fixture())
        baseline = subject._phase_graph(value)
        calls = []

        def observe(frame, event, arg):
            if event == "call" and frame.f_code is subject._phase_graph.__code__:
                calls.append(True)

        previous = sys.getprofile()
        try:
            sys.setprofile(observe)
            self.assertTrue(matches(value, baseline))
            value["Config_writes"] = (1,)
            self.assertFalse(matches(value, baseline))
        finally:
            sys.setprofile(previous)
        self.assertEqual(calls, [], "ordinary Tail model records should not rebuild a snapshot")
        self.assertEqual(len(baseline[1]), 23508)

    @staticmethod
    def substantial_graph():
        @dataclass
        class Record:
            index: int
            payload: object

        shared = [None, True, b"data"]
        return [Record(i, {"tag": i, "shared": shared, "values": [i, -i]}) for i in range(4000)]

    def test_canonical_comparison_peak_allocation_is_under_half_the_writer(self):
        value = self.substantial_graph()
        baseline = subject._phase_graph(value)
        self.assertEqual(len(baseline[1]), 12002)

        def peak(compare):
            gc.collect()
            tracemalloc.start()
            try:
                self.assertTrue(compare(value, baseline))
                return tracemalloc.get_traced_memory()[1]
            finally:
                tracemalloc.stop()

        legacy = peak(lambda v, b: subject._phase_graph(v) == b)
        candidate = peak(matches)
        self.assertLess(candidate, legacy * 0.5, (legacy, candidate))


if __name__ == "__main__":
    unittest.main()
