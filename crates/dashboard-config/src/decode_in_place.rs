//! Private seeds for the current Postcard sequence representation only.
//! JSON retains derived serde. Field order must match `model::Config`.
use core::fmt;

use heapless::Vec;
use serde::de::{DeserializeSeed, Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::Config;

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
];

pub(crate) struct ConfigSeed<'a>(pub(crate) &'a mut Config);

impl<'de> DeserializeSeed<'de> for ConfigSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<(), D::Error> {
        deserializer.deserialize_struct("Config", FIELDS, self)
    }
}

impl<'de> Visitor<'de> for ConfigSeed<'_> {
    type Value = ();

    fn expecting(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        formatter.write_str("the current Postcard Config sequence")
    }

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
        Ok(())
    }
}

fn next<'de, T: Deserialize<'de>, A: SeqAccess<'de>>(
    seq: &mut A,
    field: &'static str,
) -> Result<T, A::Error> {
    seq.next_element()?.ok_or_else(|| A::Error::missing_field(field))
}

// Separate bounded View and Widget visitor instantiations reuse this seed.
// Only an individual element is returned by serde; no full vector is built.
struct VecSeed<'a, T, const N: usize>(&'a mut Vec<T, N>);

impl<'de, T: Deserialize<'de>, const N: usize> DeserializeSeed<'de> for VecSeed<'_, T, N> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for VecSeed<'_, T, N> {
    type Value = ();

    fn expecting(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        formatter.write_str("a bounded sequence")
    }

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
