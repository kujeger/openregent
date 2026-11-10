//! What a new game is made of with no save (`world::new_map`), held against
//! the start trace's save. The whole world, made from the generator's
//! ground, is `regent_core/tests/fresh_generated.rs`.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Library;
use regent_core::data::desc;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Image;
use regent_core::sim::{clearing, movement, placement};

/// `result`'s value, else a panic naming descriptor `code`.
fn parsed<T>(code: FourCc, result: regent_core::data::error::Result<T>) -> T {
    result.unwrap_or_else(|e| panic!("{code}: {e}"))
}

/// Setup's tables skip a record or art that will not parse and default
/// its field (`sim::setup`); none in either of HD's datasets is skipped.
#[test]
fn every_record_the_tables_read_parses() {
    let hd = common::hd();

    for sets in [Datasets::Base, Datasets::Expansion] {
        let files = Install::new(&hd, Bytecode::Install)
            .files(sets)
            .expect("the dataset's files");
        let library = Library::from_files(&files.archives).expect("archives");
        let catalogue = library.catalogue().expect("a catalogue");
        let part = |data| desc::Part::parse(data).expect("a descriptor");

        for (_, r) in catalogue.tagged(b"DACT") {
            desc::Action::parse(&part(r.data)).expect("an action");
        }

        for (_, r) in catalogue.tagged(b"DMOV") {
            movement::Class::parse(r.data).expect("a movement class");
        }

        for (_, r) in catalogue.tagged(b"DUNT") {
            let part = part(r.data);
            let head = desc::Head::parse(part.find(b"HEAD").expect("HEAD").body).expect("a head");
            let code = head.id;
            let unit = Unit::find(&catalogue, code)
                .expect("found")
                .unwrap_or_else(|e| panic!("{code}: {e}"));
            parsed(code, desc::Unit::parse(&part));
            parsed(code, placement::taken(&catalogue, &unit));
            parsed(code, placement::exits(&catalogue, &unit));
            parsed(code, placement::shape(&catalogue, &unit));
            parsed(code, placement::pad_rect(&catalogue, &unit));
            parsed(code, placement::pixel_rect(&catalogue, &unit));
            parsed(code, clearing::ring(&catalogue, &unit, clearing::MARGIN));

            if let Some(at) = unit.art.as_ref().and_then(|a| a.at) {
                let record = catalogue.record(b"IMAG", at).expect("its art");

                parsed(code, Image::parse(record.data));
            }
        }
    }
}
