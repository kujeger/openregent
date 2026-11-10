//! The original's autosave notice, read from the dataset.

mod common;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::catalogue::Library;
use regent_core::data::text::Text;
use regent_core::view::menu::autosave::NOTICE;

#[test]
fn the_autosave_notice_is_gmtx_335() {
    let files = Install::new(&common::hd(), Bytecode::Install)
        .files(Datasets::Base)
        .map(|f| f.archives)
        .expect("the dataset's files");
    let library = Library::from_files(&files).expect("the archives read");
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);

    assert_eq!(text.gmtx(NOTICE.index).as_deref(), Some("Autosaving game."));
}
