//! Private seeds for the current Postcard sequence representation only.
//! JSON retains derived serde. Field order must match `model::Config`.
use core::fmt;

use heapless::Vec;
use serde::de::{DeserializeSeed, Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::Config;

/// Current Config declaration order; additions/reordering must match model, codec and migration tests.
const FIELDS: &[&str] = &[
    "version",
    "catalogue_version",
    "boot_view",
    "views",
    "widgets",
    "sources",
    "warnings",
    "fuel",
    "warning_colors",
    "controls",
    "history_seconds",
    "reduced_motion",
    "led",
    "orientation",
    "ble",
];

/// In-place sequence seed; errors may leave partial mutation, so storage must remain unpublished.
pub(crate) struct ConfigSeed<'a>(
    /// Exclusively borrowed initialized candidate, not necessarily valid.
    pub(crate) &'a mut Config,
);

impl<'de> DeserializeSeed<'de> for ConfigSeed<'_> {
    /// Decoding mutates borrowed storage rather than returning another full model.
    type Value = ();

    /// Decode from sequence-capable `deserializer`; returns its shape/field errors with possible partial writes.
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<(), D::Error> {
        deserializer.deserialize_struct("Config", FIELDS, self)
    }
}

impl<'de> Visitor<'de> for ConfigSeed<'_> {
    /// Successful in-place mutation has no owned model result.
    type Value = ();

    /// Write the required wire-shape description to `formatter`; returns formatting sink errors.
    fn expecting(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        formatter.write_str("the current Postcard Config sequence")
    }

    /// Fill fields in declaration order from `seq`, returning the first decode/missing-field error.
    /// Prior writes remain on error; envelope consumption/canonicalization and validation occur afterward.
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<(), A::Error> {
        self.0.version = next(&mut seq, "version")?;
        self.0.catalogue_version = next(&mut seq, "catalogue_version")?;
        self.0.boot_view = next(&mut seq, "boot_view")?;
        seq.next_element_seed(VecSeed(&mut self.0.views))?
            .ok_or_else(|| A::Error::missing_field("views"))?;
        seq.next_element_seed(VecSeed(&mut self.0.widgets))?
            .ok_or_else(|| A::Error::missing_field("widgets"))?;
        self.0.sources = next(&mut seq, "sources")?;
        self.0.warnings = next(&mut seq, "warnings")?;
        self.0.fuel = next(&mut seq, "fuel")?;
        self.0.warning_colors = next(&mut seq, "warning_colors")?;
        self.0.controls = next(&mut seq, "controls")?;
        self.0.history_seconds = next(&mut seq, "history_seconds")?;
        self.0.reduced_motion = next(&mut seq, "reduced_motion")?;
        self.0.led = next(&mut seq, "led")?;
        self.0.orientation = next(&mut seq, "orientation")?;
        self.0.ble = next(&mut seq, "ble")?;
        Ok(())
    }
}

/// Return the next `T` from `seq`; absence is reported with the exact static `field` name.
/// Underlying decoding errors propagate unchanged.
fn next<'de, T: Deserialize<'de>, A: SeqAccess<'de>>(
    seq: &mut A,
    field: &'static str,
) -> Result<T, A::Error> {
    seq.next_element()?.ok_or_else(|| A::Error::missing_field(field))
}

/// Shared bounded View/Widget seed; serde returns one `T` at a time, never a full vector temporary.
struct VecSeed<'a, T, const N: usize>(
    /// Initialized destination with capacity `N`; decoding may clear/fill it before failure.
    &'a mut Vec<T, N>,
);

impl<'de, T: Deserialize<'de>, const N: usize> DeserializeSeed<'de> for VecSeed<'_, T, N> {
    /// No owned vector is returned.
    type Value = ();

    /// Decode a sequence through `deserializer`, returning its errors with possible destination mutation.
    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for VecSeed<'_, T, N> {
    /// Success leaves the decoded elements in the borrowed vector.
    type Value = ();

    /// Write the bounded-sequence expectation to `formatter`; returns formatting errors.
    fn expecting(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        formatter.write_str("a bounded sequence")
    }

    /// Replace the vector from `seq`, returning decode/capacity errors rather than truncating.
    /// Oversized size hints reject before clearing. Otherwise old contents are cleared and
    /// a valid prefix can remain on failure; a missing/lying hint never bypasses capacity checks.
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<(), A::Error> {
        if seq.size_hint().is_some_and(|len| len > N) {
            return Err(A::Error::custom("sequence exceeds capacity"));
        }
        self.0.clear();
        while let Some(element) = seq.next_element()? {
            self.0
                .push(element)
                .map_err(|_| A::Error::custom("sequence exceeds capacity"))?;
        }
        Ok(())
    }
}
