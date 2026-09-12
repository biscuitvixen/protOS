//! Input vocabulary shared by every producer and by the rig.
//!
//! Every control signal facegen consumes is one named float. This module
//! holds the flat table of those names, the OSC address each arrives on,
//! and the store the receiver threads write into. Producers speak
//! addresses; the rig wants dense indices, so the table is fixed at
//! compile time and an address resolves to an index once per message.
//!
//! Table order: the first 45 entries are Project Babble's blendshapes in
//! the order Babble emits them (BabbleApp/osc.py at v2.0.7, identical in
//! Baballonia rc6), so the index doubles as Babble's output index. Then
//! the 22 ARKit `ARFaceAnchor.BlendShapeLocation` names Babble cannot
//! produce (eyes, brows, cheekPuff, cheekSquint), then the eight
//! /protos/eye channels and the voice level from the protOS bus.
//!
//! Left and Right in a blendshape name are the wearer's own left and
//! right, as in ARKit, and select which half-face rig the shape drives.
//! In jawLeft/Right, mouthLeft/Right, tongueLeft/Right and
//! tongueTwistLeft/Right the suffix is a direction of movement, not a
//! side, so those carry `Side::Both`. Values are clamped to [0, 1]
//! except eye gaze x and y, which are [-1, 1] (+x wearer's right, +y up).

use std::collections::HashMap;
use std::sync::LazyLock;
use web_time::Instant;

/// Which facial feature an input drives. The rig groups parameters by
/// feature; the web harness groups sliders the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feature {
    Eye,
    Brow,
    Jaw,
    Mouth,
    Cheek,
    Nose,
    Tongue,
    Voice,
}

/// Which half-face rig an input belongs to. `Both` inputs drive the two
/// rigs with the same weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Both,
}

/// Value range an input is clamped to on write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    /// [0, 1]
    Unit,
    /// [-1, 1]
    Signed,
}

impl Range {
    pub fn clamp(self, value: f32) -> f32 {
        match self {
            Range::Unit => value.clamp(0.0, 1.0),
            Range::Signed => value.clamp(-1.0, 1.0),
        }
    }
}

/// Which contract an input comes from. Babble names are also valid
/// ARKit names where the two overlap; `ArKit` marks the ARKit-only set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Babble,
    ArKit,
    ProtosEye,
    ProtosVoice,
}

/// One row of the input table.
#[derive(Clone, Copy, Debug)]
pub struct InputSpec {
    /// camelCase name used in face TOML morph rows and on the web page.
    pub name: &'static str,
    /// OSC address the value arrives on, single float argument.
    pub address: &'static str,
    pub feature: Feature,
    pub side: Side,
    pub range: Range,
    pub source: Source,
    /// Value before any producer has written the channel. Lids start
    /// open and pupils mid-size so a missing eye tracker leaves a
    /// normal face; every blendshape starts at rest.
    pub initial: f32,
}

/// Dense index into [`INPUTS`] and [`InputStore`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputId(u8);

impl InputId {
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn spec(self) -> &'static InputSpec {
        &INPUTS[self.index()]
    }
}

pub const BABBLE_COUNT: usize = 45;
pub const ARKIT_EXTRA_COUNT: usize = 22;
pub const PROTOS_EYE_COUNT: usize = 8;
pub const PROTOS_VOICE_COUNT: usize = 1;
pub const INPUT_COUNT: usize =
    BABBLE_COUNT + ARKIT_EXTRA_COUNT + PROTOS_EYE_COUNT + PROTOS_VOICE_COUNT;

/// A blendshape row: the OSC address is the bare name with a leading
/// slash, which is Babble's wire format with its default empty prefix.
macro_rules! shape {
    ($name:literal, $feature:ident, $side:ident, $source:ident) => {
        InputSpec {
            name: $name,
            address: concat!("/", $name),
            feature: Feature::$feature,
            side: Side::$side,
            range: Range::Unit,
            source: Source::$source,
            initial: 0.0,
        }
    };
}

/// A protOS bus row with an explicit address, range and initial value.
macro_rules! bus {
    ($name:literal, $address:literal, $feature:ident, $side:ident, $range:ident, $source:ident, $initial:literal) => {
        InputSpec {
            name: $name,
            address: $address,
            feature: Feature::$feature,
            side: Side::$side,
            range: Range::$range,
            source: Source::$source,
            initial: $initial,
        }
    };
}

pub static INPUTS: [InputSpec; INPUT_COUNT] = [
    // Babble, in emitted order (index == Babble output index).
    shape!("cheekPuffLeft", Cheek, Left, Babble),
    shape!("cheekPuffRight", Cheek, Right, Babble),
    shape!("cheekSuckLeft", Cheek, Left, Babble),
    shape!("cheekSuckRight", Cheek, Right, Babble),
    shape!("jawOpen", Jaw, Both, Babble),
    shape!("jawForward", Jaw, Both, Babble),
    // Left/Right here is a direction of movement, not a side: the shape
    // drives both half-face rigs and the rig applies the sign per side.
    shape!("jawLeft", Jaw, Both, Babble),
    shape!("jawRight", Jaw, Both, Babble),
    shape!("noseSneerLeft", Nose, Left, Babble),
    shape!("noseSneerRight", Nose, Right, Babble),
    shape!("mouthFunnel", Mouth, Both, Babble),
    shape!("mouthPucker", Mouth, Both, Babble),
    // Left/Right here is a direction of movement, not a side: the shape
    // drives both half-face rigs and the rig applies the sign per side.
    shape!("mouthLeft", Mouth, Both, Babble),
    shape!("mouthRight", Mouth, Both, Babble),
    shape!("mouthRollUpper", Mouth, Both, Babble),
    shape!("mouthRollLower", Mouth, Both, Babble),
    shape!("mouthShrugUpper", Mouth, Both, Babble),
    shape!("mouthShrugLower", Mouth, Both, Babble),
    shape!("mouthClose", Mouth, Both, Babble),
    shape!("mouthSmileLeft", Mouth, Left, Babble),
    shape!("mouthSmileRight", Mouth, Right, Babble),
    shape!("mouthFrownLeft", Mouth, Left, Babble),
    shape!("mouthFrownRight", Mouth, Right, Babble),
    shape!("mouthDimpleLeft", Mouth, Left, Babble),
    shape!("mouthDimpleRight", Mouth, Right, Babble),
    shape!("mouthUpperUpLeft", Mouth, Left, Babble),
    shape!("mouthUpperUpRight", Mouth, Right, Babble),
    shape!("mouthLowerDownLeft", Mouth, Left, Babble),
    shape!("mouthLowerDownRight", Mouth, Right, Babble),
    shape!("mouthPressLeft", Mouth, Left, Babble),
    shape!("mouthPressRight", Mouth, Right, Babble),
    shape!("mouthStretchLeft", Mouth, Left, Babble),
    shape!("mouthStretchRight", Mouth, Right, Babble),
    shape!("tongueOut", Tongue, Both, Babble),
    shape!("tongueUp", Tongue, Both, Babble),
    shape!("tongueDown", Tongue, Both, Babble),
    // Left/Right here is a direction of movement, not a side: the shape
    // drives both half-face rigs and the rig applies the sign per side.
    shape!("tongueLeft", Tongue, Both, Babble),
    shape!("tongueRight", Tongue, Both, Babble),
    shape!("tongueRoll", Tongue, Both, Babble),
    shape!("tongueBendDown", Tongue, Both, Babble),
    shape!("tongueCurlUp", Tongue, Both, Babble),
    shape!("tongueSquish", Tongue, Both, Babble),
    shape!("tongueFlat", Tongue, Both, Babble),
    // Left/Right here is a direction of movement, not a side: the shape
    // drives both half-face rigs and the rig applies the sign per side.
    shape!("tongueTwistLeft", Tongue, Both, Babble),
    shape!("tongueTwistRight", Tongue, Both, Babble),
    // ARKit names Babble does not produce.
    shape!("eyeBlinkLeft", Eye, Left, ArKit),
    shape!("eyeLookDownLeft", Eye, Left, ArKit),
    shape!("eyeLookInLeft", Eye, Left, ArKit),
    shape!("eyeLookOutLeft", Eye, Left, ArKit),
    shape!("eyeLookUpLeft", Eye, Left, ArKit),
    shape!("eyeSquintLeft", Eye, Left, ArKit),
    shape!("eyeWideLeft", Eye, Left, ArKit),
    shape!("eyeBlinkRight", Eye, Right, ArKit),
    shape!("eyeLookDownRight", Eye, Right, ArKit),
    shape!("eyeLookInRight", Eye, Right, ArKit),
    shape!("eyeLookOutRight", Eye, Right, ArKit),
    shape!("eyeLookUpRight", Eye, Right, ArKit),
    shape!("eyeSquintRight", Eye, Right, ArKit),
    shape!("eyeWideRight", Eye, Right, ArKit),
    shape!("browDownLeft", Brow, Left, ArKit),
    shape!("browDownRight", Brow, Right, ArKit),
    shape!("browInnerUp", Brow, Both, ArKit),
    shape!("browOuterUpLeft", Brow, Left, ArKit),
    shape!("browOuterUpRight", Brow, Right, ArKit),
    shape!("cheekPuff", Cheek, Both, ArKit),
    shape!("cheekSquintLeft", Cheek, Left, ArKit),
    shape!("cheekSquintRight", Cheek, Right, ArKit),
    // protOS bus: eye tracking and voice level.
    bus!(
        "eyeLeftX",
        "/protos/eye/left/x",
        Eye,
        Left,
        Signed,
        ProtosEye,
        0.0
    ),
    bus!(
        "eyeLeftY",
        "/protos/eye/left/y",
        Eye,
        Left,
        Signed,
        ProtosEye,
        0.0
    ),
    bus!(
        "eyeLeftLid",
        "/protos/eye/left/lid",
        Eye,
        Left,
        Unit,
        ProtosEye,
        1.0
    ),
    bus!(
        "eyeLeftPupil",
        "/protos/eye/left/pupil",
        Eye,
        Left,
        Unit,
        ProtosEye,
        0.5
    ),
    bus!(
        "eyeRightX",
        "/protos/eye/right/x",
        Eye,
        Right,
        Signed,
        ProtosEye,
        0.0
    ),
    bus!(
        "eyeRightY",
        "/protos/eye/right/y",
        Eye,
        Right,
        Signed,
        ProtosEye,
        0.0
    ),
    bus!(
        "eyeRightLid",
        "/protos/eye/right/lid",
        Eye,
        Right,
        Unit,
        ProtosEye,
        1.0
    ),
    bus!(
        "eyeRightPupil",
        "/protos/eye/right/pupil",
        Eye,
        Right,
        Unit,
        ProtosEye,
        0.5
    ),
    bus!(
        "voiceLevel",
        "/protos/voice/level",
        Voice,
        Both,
        Unit,
        ProtosVoice,
        0.0
    ),
];

/// Address and name lookups, built once on first use.
struct Index {
    by_address: HashMap<&'static str, InputId>,
    by_name: HashMap<&'static str, InputId>,
}

static INDEX: LazyLock<Index> = LazyLock::new(|| {
    let mut by_address = HashMap::with_capacity(INPUT_COUNT);
    let mut by_name = HashMap::with_capacity(INPUT_COUNT);
    for (i, spec) in INPUTS.iter().enumerate() {
        let id = InputId(u8::try_from(i).expect("input table fits in u8"));
        by_address.insert(spec.address, id);
        by_name.insert(spec.name, id);
    }
    Index {
        by_address,
        by_name,
    }
});

/// Resolve an OSC address such as `/jawOpen` or `/protos/eye/left/x`.
pub fn lookup_address(address: &str) -> Option<InputId> {
    INDEX.by_address.get(address).copied()
}

/// Resolve a name such as `jawOpen` or `eyeLeftX`.
pub fn lookup_name(name: &str) -> Option<InputId> {
    INDEX.by_name.get(name).copied()
}

/// All input ids in table order.
pub fn all_ids() -> impl Iterator<Item = InputId> {
    (0..INPUT_COUNT).map(|i| InputId(i as u8))
}

/// Latest value of every input plus when it was last written.
///
/// Written by the OSC receiver and the web harness, read by the render
/// loop. Stale detection is the reader's job: a producer that dies
/// leaves its channels frozen at their last value with an old timestamp.
#[derive(Clone, Debug)]
pub struct InputStore {
    values: [f32; INPUT_COUNT],
    last_seen: [Option<Instant>; INPUT_COUNT],
}

impl Default for InputStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InputStore {
    pub fn new() -> Self {
        let mut values = [0.0; INPUT_COUNT];
        for (v, spec) in values.iter_mut().zip(INPUTS.iter()) {
            *v = spec.initial;
        }
        Self {
            values,
            last_seen: [None; INPUT_COUNT],
        }
    }

    /// Reset every channel to its initial value.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Store a value, clamped to the input's range. `now` is injected so
    /// tests control time.
    pub fn set(&mut self, id: InputId, value: f32, now: Instant) {
        let i = id.index();
        self.values[i] = INPUTS[i].range.clamp(value);
        self.last_seen[i] = Some(now);
    }

    /// Store by OSC address. Returns false for an unknown address, which
    /// the caller logs once rather than per message.
    pub fn set_by_address(&mut self, address: &str, value: f32, now: Instant) -> bool {
        match lookup_address(address) {
            Some(id) => {
                self.set(id, value, now);
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: InputId) -> f32 {
        self.values[id.index()]
    }

    pub fn last_seen(&self, id: InputId) -> Option<Instant> {
        self.last_seen[id.index()]
    }

    /// Dense view for the rig, indexed by [`InputId::index`].
    pub fn values(&self) -> &[f32; INPUT_COUNT] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::Duration;

    // Apple ARFaceAnchor.BlendShapeLocation, all 52 names, read from
    // developer.apple.com on 2026-09-10 in Apple's own topic order.
    const ARKIT_52: [&str; 52] = [
        "eyeBlinkLeft",
        "eyeLookDownLeft",
        "eyeLookInLeft",
        "eyeLookOutLeft",
        "eyeLookUpLeft",
        "eyeSquintLeft",
        "eyeWideLeft",
        "eyeBlinkRight",
        "eyeLookDownRight",
        "eyeLookInRight",
        "eyeLookOutRight",
        "eyeLookUpRight",
        "eyeSquintRight",
        "eyeWideRight",
        "jawForward",
        "jawLeft",
        "jawRight",
        "jawOpen",
        "mouthClose",
        "mouthFunnel",
        "mouthPucker",
        "mouthLeft",
        "mouthRight",
        "mouthSmileLeft",
        "mouthSmileRight",
        "mouthFrownLeft",
        "mouthFrownRight",
        "mouthDimpleLeft",
        "mouthDimpleRight",
        "mouthStretchLeft",
        "mouthStretchRight",
        "mouthRollLower",
        "mouthRollUpper",
        "mouthShrugLower",
        "mouthShrugUpper",
        "mouthPressLeft",
        "mouthPressRight",
        "mouthLowerDownLeft",
        "mouthLowerDownRight",
        "mouthUpperUpLeft",
        "mouthUpperUpRight",
        "browDownLeft",
        "browDownRight",
        "browInnerUp",
        "browOuterUpLeft",
        "browOuterUpRight",
        "cheekPuff",
        "cheekSquintLeft",
        "cheekSquintRight",
        "noseSneerLeft",
        "noseSneerRight",
        "tongueOut",
    ];

    #[test]
    fn the_table_has_the_documented_section_sizes() {
        let count = |source| INPUTS.iter().filter(|s| s.source == source).count();
        assert_eq!(
            count(Source::Babble),
            BABBLE_COUNT,
            "Babble section size changed"
        );
        assert_eq!(
            count(Source::ArKit),
            ARKIT_EXTRA_COUNT,
            "ARKit-only section size changed"
        );
        assert_eq!(
            count(Source::ProtosEye),
            PROTOS_EYE_COUNT,
            "eye section size changed"
        );
        assert_eq!(
            count(Source::ProtosVoice),
            PROTOS_VOICE_COUNT,
            "voice section size changed"
        );
    }

    #[test]
    fn babble_entries_come_first_in_babble_emitted_order() {
        // BabbleApp/osc.py at v2.0.7, output_osc index order.
        let first_and_last = [
            (0, "cheekPuffLeft"),
            (4, "jawOpen"),
            (33, "tongueOut"),
            (44, "tongueTwistRight"),
        ];
        for (index, name) in first_and_last {
            assert_eq!(
                INPUTS[index].name, name,
                "Babble index {index} is not {name}"
            );
            assert_eq!(
                INPUTS[index].source,
                Source::Babble,
                "index {index} is not a Babble row"
            );
        }
    }

    #[test]
    fn names_and_addresses_are_unique() {
        let names: HashSet<_> = INPUTS.iter().map(|s| s.name).collect();
        let addresses: HashSet<_> = INPUTS.iter().map(|s| s.address).collect();
        assert_eq!(
            names.len(),
            INPUT_COUNT,
            "duplicate input name in the table"
        );
        assert_eq!(
            addresses.len(),
            INPUT_COUNT,
            "duplicate OSC address in the table"
        );
    }

    #[test]
    fn every_arkit_name_is_in_the_vocabulary_and_nothing_else_claims_arkit() {
        for name in ARKIT_52 {
            assert!(
                lookup_name(name).is_some(),
                "ARKit name {name} missing from the table"
            );
        }
        let arkit: HashSet<_> = ARKIT_52.into_iter().collect();
        for spec in INPUTS.iter().filter(|s| s.source == Source::ArKit) {
            assert!(
                arkit.contains(spec.name),
                "{} is marked ArKit but is not an ARKit name",
                spec.name
            );
        }
        let shared = INPUTS
            .iter()
            .filter(|s| s.source == Source::Babble && arkit.contains(s.name))
            .count();
        assert_eq!(shared, 30, "Babble/ARKit overlap should be 30 names");
    }

    #[test]
    fn babble_addresses_are_the_bare_name_with_a_slash() {
        for spec in INPUTS
            .iter()
            .filter(|s| matches!(s.source, Source::Babble | Source::ArKit))
        {
            assert_eq!(
                spec.address,
                format!("/{}", spec.name),
                "address for {} is not bare",
                spec.name
            );
        }
    }

    #[test]
    fn side_follows_the_name_suffix_except_for_direction_words() {
        let directions = [
            "jawLeft",
            "jawRight",
            "mouthLeft",
            "mouthRight",
            "tongueLeft",
            "tongueRight",
            "tongueTwistLeft",
            "tongueTwistRight",
        ];
        for spec in INPUTS.iter() {
            let expected = if directions.contains(&spec.name) {
                Side::Both
            } else if spec.name.ends_with("Left") || spec.name.starts_with("eyeLeft") {
                Side::Left
            } else if spec.name.ends_with("Right") || spec.name.starts_with("eyeRight") {
                Side::Right
            } else {
                Side::Both
            };
            assert_eq!(
                spec.side, expected,
                "side of {} disagrees with its name",
                spec.name
            );
        }
    }

    #[test]
    fn lookups_round_trip_by_address_and_by_name() {
        for id in all_ids() {
            let spec = id.spec();
            assert_eq!(
                lookup_address(spec.address),
                Some(id),
                "address lookup failed for {}",
                spec.name
            );
            assert_eq!(
                lookup_name(spec.name),
                Some(id),
                "name lookup failed for {}",
                spec.name
            );
        }
        assert_eq!(
            lookup_address("/avatar/parameters/jawOpen"),
            None,
            "prefixed address must not resolve"
        );
        assert_eq!(
            lookup_name("JawOpen"),
            None,
            "lookup must be case sensitive"
        );
    }

    #[test]
    fn the_store_clamps_to_the_input_range_and_records_the_time() {
        let mut store = InputStore::new();
        let t0 = Instant::now();
        let jaw = lookup_name("jawOpen").unwrap();
        let gaze = lookup_name("eyeLeftX").unwrap();
        store.set(jaw, 1.7, t0);
        store.set(gaze, -3.0, t0 + Duration::from_millis(5));
        assert_eq!(store.get(jaw), 1.0, "unit input not clamped to 1");
        assert_eq!(store.get(gaze), -1.0, "signed input not clamped to -1");
        assert_eq!(
            store.last_seen(jaw),
            Some(t0),
            "jawOpen timestamp not recorded"
        );
        assert_eq!(
            store.last_seen(gaze),
            Some(t0 + Duration::from_millis(5)),
            "gaze timestamp not recorded"
        );
        assert_eq!(
            store.last_seen(lookup_name("voiceLevel").unwrap()),
            None,
            "untouched input should have no timestamp"
        );
    }

    #[test]
    fn a_fresh_store_has_open_lids_and_mid_pupils_and_resting_shapes() {
        let store = InputStore::new();
        assert_eq!(
            store.get(lookup_name("eyeLeftLid").unwrap()),
            1.0,
            "lid starts open"
        );
        assert_eq!(
            store.get(lookup_name("eyeRightPupil").unwrap()),
            0.5,
            "pupil starts mid-size"
        );
        assert_eq!(
            store.get(lookup_name("jawOpen").unwrap()),
            0.0,
            "jaw starts closed"
        );
    }

    #[test]
    fn setting_by_address_rejects_unknown_addresses_without_touching_the_store() {
        let mut store = InputStore::new();
        let now = Instant::now();
        assert!(
            store.set_by_address("/jawOpen", 0.5, now),
            "known address rejected"
        );
        assert!(
            !store.set_by_address("/nope", 0.5, now),
            "unknown address accepted"
        );
        let changed = store
            .values()
            .iter()
            .zip(INPUTS.iter())
            .filter(|(v, s)| **v != s.initial)
            .count();
        assert_eq!(
            changed, 1,
            "only jawOpen should have moved from its initial value"
        );
    }
}
