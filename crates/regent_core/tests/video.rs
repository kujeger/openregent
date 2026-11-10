//! The `PICT` Bink videos decode, with the vendored `bink` crate.
//!
//! Needs `MAJESTY_HD` (environment or `.env`).

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use regent_core::data::{cam, payload};

/// The archives holding videos: every `.cam`, and the cinematics'
/// `.dat`s (`cinedata2.dat`, `cinedata3.dat` and their `mx_` twins).
fn video_archives(root: &Path) -> Vec<std::path::PathBuf> {
    let cinematics = common::walk(root, &["dat"]).into_iter().filter(|p| {
        p.file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|f| f.to_ascii_lowercase().contains("cinedata"))
    });

    common::archives(root)
        .into_iter()
        .chain(cinematics)
        .collect()
}

/// Every kind-7 `PICT` under `root`, by archive file name and record id:
/// the Bink file.
fn videos(root: &Path) -> BTreeMap<(String, String), Vec<u8>> {
    let mut found = BTreeMap::new();

    for path in video_archives(root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();
        let name = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();

        for record in archive
            .chunk_by_tag(b"PICT")
            .iter()
            .flat_map(|c| &c.records)
        {
            if let payload::Pict::Video { file, .. } = payload::Pict::parse(record.data).unwrap() {
                found.insert((name.clone(), record.id.to_string()), file.to_vec());
            }
        }
    }

    found
}

/// Whether `archive`'s file name has extension `ext`, any case.
fn kind(archive: &str, ext: &str) -> bool {
    Path::new(archive)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// FNV-1a, 64 bits: a frame's hash with no hashing dependency.
fn fnv(bytes: &[u8]) -> u64 {
    regent_core::data::fnv::fold(regent_core::data::fnv::START, bytes.iter().copied())
}

/// The size a video is stored at. The `.cam` ones: only `MV01` and
/// `MV06` (the credits) are 640×480; the 31 `QM*` (the briefing porthole)
/// 260×260. The `cinedata2.dat` pair (`WhichCinematic` 0) is stored small,
/// its header asking Bink to double it.
fn stored(archive: &str, id: &str) -> (usize, usize) {
    match (archive, id) {
        ("cinedata2.dat", _) => (320, 240),
        ("mx_cinedata2.dat", _) => (640, 240),
        (_, id) if id.starts_with("QM") => (260, 260),
        _ => (640, 480),
    }
}

/// Each video decodes to its header's frame count without
/// an error; all run at 15 frames a second. The `.cam` videos have no
/// audio track; the `.dat` ones (intro and Cinematic) one each.
#[test]
fn every_bink_record_decodes_to_its_header_frame_count() {
    let hd = videos(&common::hd());

    assert_eq!(hd.len(), 39, "33 in .cam files, 6 in .dat files");

    let mut frames = 0;

    for ((archive, id), file) in &hd {
        let mut video =
            bink::Video::open(file.as_slice()).unwrap_or_else(|e| panic!("{archive} {id}: {e}"));
        let header = *video.header();

        assert_eq!(
            (header.width, header.height),
            stored(archive, id),
            "{archive} {id}"
        );
        assert_eq!(
            (header.rate_num, header.rate_den),
            (15, 1),
            "{archive} {id}"
        );
        let tracks = usize::from(kind(archive, "dat"));
        assert_eq!(header.audio_tracks, tracks, "{archive} {id}");

        for n in 0.. {
            match video.next_frame() {
                Some(frame) => {
                    _ = frame.unwrap_or_else(|e| panic!("{archive} {id} frame {n}: {e}"));
                }
                None => break,
            }
        }

        assert_eq!(video.position(), header.frames, "{archive} {id}");

        if kind(archive, "cam") {
            frames += header.frames;
        }
    }

    assert_eq!(frames, 19_153, "the .cam videos' frames");
}

/// Frames hash as ffmpeg decodes them (`-pix_fmt yuv420p`, FNV-1a 64 of
/// the raw frame): every revision `f`, `h` and `i`, the stored-small
/// ones, first and last frames.
#[test]
fn a_few_frames_hash_as_ffmpeg_decodes_them() {
    let hd = videos(&common::hd());

    let pinned: [(&str, &str, usize, u64); 12] = [
        ("cinedata1.cam", "MV01", 0, 0x79d1_6b8a_83e3_e8ea),
        ("cinedata1.cam", "MV01", 640, 0xeeb1_539c_9f69_88d8),
        ("cinedata1.cam", "MV01", 1280, 0x84ea_1aa4_7fe1_4e89),
        ("mx_cinedata1.cam", "MV06", 1000, 0x3ef4_c943_bc3c_ac22),
        ("cinedata1.cam", "QM13", 100, 0x2ac9_5224_e954_6532),
        ("mx_cinedata1.cam", "QM20", 300, 0x7a8e_2fe6_475c_4297),
        ("mx_cinedata1.cam", "QM20", 649, 0x6537_643b_ff68_c755),
        ("cinedata3.dat", "MV02", 100, 0x5177_4fe0_5c34_5143),
        ("cinedata3.dat", "MV04", 1000, 0x1d5c_fbf7_534a_aa7d),
        ("cinedata2.dat", "MV04", 500, 0x07c3_c622_2459_b300),
        ("mx_cinedata3.dat", "MV07", 300, 0x4a7b_89a0_21ed_7436),
        ("mx_cinedata2.dat", "MV07", 200, 0x7701_011d_2905_c776),
    ];

    for (archive, id, at, want) in pinned {
        let key = (archive.to_owned(), id.to_owned());
        let mut video = bink::Video::open(hd[&key].as_slice()).unwrap();

        for _ in 0..at {
            video.next_frame().unwrap().unwrap();
        }

        let frame = video.next_frame().unwrap().unwrap();
        assert_eq!(fnv(&frame.yuv420p()), want, "{archive} {id} frame {at}");
    }
}

/// Each screen's videos are where its reader looks: CREDITS' and the
/// briefings' in the `.cam` archives, the intro's and CINEMATIC's in
/// `menu::CINEMATICS`.
#[test]
fn every_screens_video_is_in_the_archive_it_is_read_from() {
    use regent_core::view::menu::quest_map::{SLOTS, video};
    use regent_core::view::menu::{CINEMATIC, CINEMATICS, CREDITS, INTRO};

    let hd = videos(&common::hd());
    let held = |archives: &dyn Fn(&str) -> bool, id: String| {
        hd.keys().any(|(archive, i)| archives(archive) && *i == id)
    };
    let cam = |archive: &str| kind(archive, "cam");
    let cinematics: Vec<String> = CINEMATICS
        .iter()
        .map(|p| p.rsplit('/').next().unwrap().to_ascii_lowercase())
        .collect();
    let cinematic = |archive: &str| cinematics.iter().any(|c| c == archive);

    assert!(CREDITS.iter().all(|id| held(&cam, id.to_string())));

    // `XQD1`'s video, and `QM31`–`QM33`, do not ship.
    let briefings = (0..SLOTS.len()).filter_map(video);
    assert_eq!(
        briefings.filter(|id| held(&cam, id.to_string())).count(),
        31
    );

    assert!(
        INTRO
            .iter()
            .chain(&CINEMATIC)
            .all(|id| held(&cinematic, id.to_string()))
    );
    assert!(CINEMATICS.iter().all(|p| common::hd().join(p).is_file()));
}

/// Each film of the Cinematics page is in the archive it says.
#[test]
fn each_film_is_in_its_archive() {
    use regent_core::view::menu::CINEMATICS;
    use regent_core::view::menu::film::Film;

    let hd = videos(&common::hd());

    for film in Film::ALL {
        let archive = std::path::Path::new(CINEMATICS[film.archive()])
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a file name")
            .to_owned();

        assert!(
            hd.contains_key(&(archive, film.id().to_string())),
            "{film:?}"
        );
    }
}

/// The expansion's ending, `MV07`, is only in the expansion's
/// cinematics, which only CINEMATIC opens: neither the datasets' `.cam`s
/// nor the base's cinematics, the archives an end panel's press finds
/// open, hold it. So in HD no end panel shows it
/// (`view::menu::end::Panel::ending`).
#[test]
fn the_endings_video_is_only_in_the_expansions_cinematics() {
    use regent_core::view::menu::end::ENDING;

    let hd = videos(&common::hd());
    let holding: Vec<&str> = hd
        .keys()
        .filter(|(_, id)| *id == ENDING.to_string())
        .map(|(archive, _)| archive.as_str())
        .collect();

    assert_eq!(holding, ["mx_cinedata2.dat", "mx_cinedata3.dat"]);
}

/// Every audio track decodes in full: the six `.dat` videos', each
/// packet giving the samples its first word states, the whole as long as
/// the video to within a frame. All RDFT; rates and channels in the
/// table.
#[test]
fn every_audio_track_decodes_to_the_samples_its_packets_state() {
    let hd = videos(&common::hd());
    let mut tracks = BTreeMap::new();

    for ((archive, id), file) in hd.iter().filter(|((a, _), _)| kind(a, "dat")) {
        let mut audio =
            bink::Audio::open(file.as_slice()).unwrap_or_else(|e| panic!("{archive} {id}: {e}"));
        let (track, header) = (*audio.track(), *audio.header());
        let (mut decoded, mut stated) = (0, 0);

        for frame in 0..header.frames {
            stated += audio.stated(frame).unwrap();
            decoded += audio
                .next_packet()
                .unwrap()
                .unwrap_or_else(|e| panic!("{archive} {id} frame {frame}: {e}"))
                .len();
        }

        assert!(audio.next_packet().is_none());
        assert_eq!(decoded, stated, "{archive} {id}");

        // Samples against frames, cross-multiplied: decoded / (channels ×
        // rate) seconds against frames / 15, to within a frame.
        let per_second = track.channels * usize::try_from(track.rate).unwrap();
        let fps = usize::try_from(header.rate_num).unwrap();
        let (heard, shown) = (decoded * fps, header.frames * per_second);
        assert!(heard.abs_diff(shown) < per_second, "{archive} {id}");

        tracks.insert(
            (archive.clone(), id.clone()),
            (track.rate, track.channels, track.dct),
        );
    }

    let want: BTreeMap<(String, String), (u32, usize, bool)> = [
        ("cinedata2.dat", "MV02", (44_100, 1, false)),
        ("cinedata2.dat", "MV04", (22_050, 1, false)),
        ("cinedata3.dat", "MV02", (44_100, 2, false)),
        ("cinedata3.dat", "MV04", (22_050, 1, false)),
        ("mx_cinedata2.dat", "MV07", (44_100, 2, false)),
        ("mx_cinedata3.dat", "MV07", (44_100, 2, false)),
    ]
    .into_iter()
    .map(|(a, i, t)| ((a.to_owned(), i.to_owned()), t))
    .collect();

    assert_eq!(tracks, want);
}

/// Two whole tracks hash as decoded here (FNV-1a 64 of the samples'
/// little-endian `f32` bytes, interleaved): a stereo and a mono one.
/// Upstream `NihAV` gives the mono samples bit for bit; its stereo
/// channels come swapped against ffmpeg's, so here channel 1 is the
/// left. Against ffmpeg the samples correlate at 0.99, a sample apart
/// (**Inferred**: neither proven to be the Bink SDK's). The RDFT's
/// tables take `sin` and `cos` from the platform's maths library, so
/// another platform may hash otherwise.
#[test]
fn two_audio_tracks_hash_as_decoded_here() {
    let hd = videos(&common::hd());

    for (id, want) in [
        ("MV02", 0x3f87_14f2_4e77_bc44),
        ("MV04", 0x1ad9_ebad_b407_6640),
    ] {
        let key = ("cinedata3.dat".to_owned(), id.to_owned());
        let mut audio = bink::Audio::open(hd[&key].as_slice()).unwrap();
        let mut bytes = Vec::new();

        while let Some(packet) = audio.next_packet() {
            bytes.extend(packet.unwrap().iter().flat_map(|s| s.to_le_bytes()));
        }

        assert_eq!(fnv(&bytes), want, "{id}");
    }
}
