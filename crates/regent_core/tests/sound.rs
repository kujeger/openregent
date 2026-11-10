//! Sound: the `WAVE` samples, the sound records' engine fields, and a new
//! game's cues.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use std::collections::BTreeMap;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::{self, SoundGroup};
use regent_core::data::fourcc::FourCc;
use regent_core::data::payload::Pcm;
use regent_core::data::settings::cases::{self, Target};
use regent_core::sim::world::{self, sound::Voice};
use regent_core::view::sound::{self, Sounds};

/// `SoundViewRadius`'s binary default is the module's.
#[test]
fn the_sound_view_radius_is_the_binarys() {
    let case = cases::GAME
        .iter()
        .find(|c| c.key == "SoundViewRadius")
        .expect("a case");

    assert!(
        matches!(case.target, Target::Float { initial, .. } if initial == sound::SOUND_VIEW_RADIUS)
    );
}

/// Every `WAVE` in HD's archives decodes: 16-bit PCM, whole frames.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_wave_is_sixteen_bit_pcm() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let mut rates: BTreeMap<(u16, u32), usize> = BTreeMap::new();

    for (_, record) in catalogue.tagged(b"WAVE") {
        let pcm = Pcm::parse(record.data).unwrap_or_else(|e| panic!("{}: {e}", record.id));

        assert_eq!(pcm.frames() * 2 * usize::from(pcm.channels), pcm.data.len());
        assert_eq!(pcm.samples().count(), pcm.data.len() / 2);
        *rates.entry((pcm.channels, pcm.rate)).or_default() += 1;
    }

    let shapes: Vec<(u16, u32)> = rates.keys().copied().collect();

    assert_eq!(
        shapes,
        [
            (1, 11_025),
            (1, 22_050),
            (1, 44_100),
            (2, 11_025),
            (2, 22_050),
            (2, 44_100)
        ]
    );
}

/// `Place_Building`'s `Begin` resolves to its wave, which decodes.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn placing_a_building_plays_a_wave() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let sounds = Sounds::of(&catalogue).expect("a sound index");
    let play = sounds
        .play(
            &catalogue,
            &Voice::Sound(world::sound::PLACE_BUILDING),
            &world::sound::Phase::Id(world::sound::BEGIN),
            None,
            0.0,
        )
        .expect("a phase");
    let at = catalogue.find(b"WAVE", play.wave).expect("its wave");
    let wave = catalogue.record(b"WAVE", at).expect("a record");
    let pcm = Pcm::parse(wave.data).expect("pcm");

    assert!(!play.looped);
    assert!(pcm.frames() > 0);
}

/// Every `DSDG` has a three-word `PRIM`: limit, interval, flags; only
/// `Advisor_Group` sets a flag.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_sound_group_is_a_limit_and_an_interval() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let groups: BTreeMap<String, (u32, u32, u32)> = catalogue
        .tagged(b"DSDG")
        .map(|(_, r)| {
            let part = desc::Part::parse(r.data).expect("a tree");
            let g = SoundGroup::parse(&part).expect("a group");

            (
                String::from_utf8_lossy(g.head.name).into_owned(),
                (g.limit, g.interval, g.flags),
            )
        })
        .collect();

    let flagged: Vec<&str> = groups
        .iter()
        .filter(|(_, g)| g.2 != 0)
        .map(|(name, _)| name.as_str())
        .collect();

    assert_eq!(groups.len(), 25);
    assert_eq!(groups["Palace_Sieged_Group"], (1, 45_000, 0));
    assert_eq!(groups["Spell_Group"], (10, 0, 0));
    assert_eq!(flagged, ["Advisor_Group"]);
}

/// An action's `SoundPhase` begin, peak and end are `PRIM + 0x28` to
/// `+ 0x30`: every SDK action's, by `DSDP` name.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn an_actions_sound_phases_are_the_sdks() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let phase_names = names(&catalogue, *b"DSDP");
    let mut compared = 0;

    for file in ["Data/M_Actions.xml", "DataMX/MX_Actions.xml"] {
        let text = std::fs::read_to_string(common::sdk().join("OriginalQuests").join(file))
            .expect("the SDK's actions");
        let doc = roxmltree::Document::parse(&text).expect("xml");

        for d in doc.descendants().filter(|n| n.has_tag_name("Description")) {
            let id = FourCc::try_from(d.attribute("ID").expect("an id").as_bytes()).expect("four");
            let xml = d.descendants().find(|n| n.has_tag_name("SoundPhase"));
            let want = ["begin", "peak", "end"].map(|k| xml.and_then(|n| n.attribute(k)));
            let phases = |at| {
                let record = catalogue.record(b"DACT", at).expect("a record");
                let part = desc::Part::parse(record.data).expect("a tree");
                let action = desc::Action::parse(&part).expect("an action");

                action.sound_phases().map(|p| {
                    p.map(|p| {
                        phase_names
                            .get(&p)
                            .cloned()
                            .unwrap_or_else(|| p.to_string())
                    })
                })
            };
            let found = catalogue.all(b"DACT", id);

            // The expansion overrides a base record: one of them is this file's.
            if found.is_empty() {
                continue;
            }

            assert!(
                found
                    .iter()
                    .any(|at| phases(*at).each_ref().map(Option::as_deref) == want),
                "{id}: {want:?}"
            );
            compared += 1;
        }
    }

    assert!(compared > 100, "{compared}");
}

fn names(catalogue: &Catalogue<'_>, tag: [u8; 4]) -> BTreeMap<FourCc, String> {
    catalogue
        .tagged(&tag)
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let head = desc::Head::parse(part.find(b"HEAD")?.body).ok()?;

            Some((head.id, String::from_utf8_lossy(head.name).into_owned()))
        })
        .collect()
}

/// A refused placing click plays `Error_Alert`'s `Begin`, and the record
/// holds the phase.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_refused_click_sounds_the_error_alert() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let sounds = Sounds::of(&catalogue).expect("a sound index");
    let (voice, phase) = sound::refused();

    assert_eq!(voice, Voice::Sound(FourCc(*b"ER01")));
    assert_eq!(phase, world::sound::Phase::Id(world::sound::BEGIN));
    assert!(sounds.play(&catalogue, &voice, &phase, None, 0.0).is_some());
}

/// The `"` key plays the selection's `EG01`, and a warrior's record holds
/// the phase.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_voice_key_plays_a_warriors_eg01() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let sounds = Sounds::of(&catalogue).expect("a sound index");
    let (voice, phase) = sound::voice();
    let warrior = FourCc(*b"AVL1");

    assert_eq!(phase, world::sound::Phase::Id(FourCc(*b"EG01")));
    assert!(
        sounds
            .play(&catalogue, &voice, &phase, Some(warrior), 0.0)
            .is_some()
    );
}
