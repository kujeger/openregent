//! Inspection tools for Majesty's data files and GPL sources, and the GPL
//! compiler (`compile`).

mod archive;
mod art;
mod bytecode;
mod config;
mod descriptor;
mod env;
mod gpl;
mod menu;
mod panel;
mod quest;
mod questmap;
mod shadows;
mod terrain;
mod uidump;
mod util;
mod view;

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use regent_core::data::desc::UnitKind;

#[derive(Parser)]
#[command(name = "regent_cli", version, about, max_term_width = 100)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Mirror of `dataset::Datasets` for `menu --dataset`.
#[derive(Clone, Copy, ValueEnum)]
enum MenuDataset {
    Base,
    Expansion,
}

impl From<MenuDataset> for regent_core::content::dataset::Datasets {
    fn from(d: MenuDataset) -> Self {
        match d {
            MenuDataset::Base => Self::Base,
            MenuDataset::Expansion => Self::Expansion,
        }
    }
}

/// Mirror of [`UnitKind`] for `--kind`; keeps clap out of the library.
#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    General,
    Projectile,
    Building,
    Character,
    Overlay,
    ParticleSystem,
}

impl From<Kind> for UnitKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::General => UnitKind::General,
            Kind::Projectile => UnitKind::Projectile,
            Kind::Building => UnitKind::Building,
            Kind::Character => UnitKind::Character,
            Kind::Overlay => UnitKind::Overlay,
            Kind::ParticleSystem => UnitKind::ParticleSystem,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Identify files by magic
    Sniff { paths: Vec<String> },

    /// One line per archive, chunk summary
    Tags { paths: Vec<String> },

    /// One line per record
    List { paths: Vec<String> },

    /// Dump every STRT table's contents
    Strings { paths: Vec<String> },

    /// Write every payload to a directory
    Extract { archive: String, dir: String },

    /// Decode every TILE, one line each
    Tiles { paths: Vec<String> },

    /// Animation sets, and one record's slots and frames
    Imag {
        /// Show this record's sets, slots and frame table
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        paths: Vec<String>,
    },

    /// D* descriptor trees, and one record's decoded fields
    Desc {
        /// Show this record's fields
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        paths: Vec<String>,
    },

    /// XML `<Descriptions>` files made into records over HD's expansion
    /// archives (`MAJESTY_HD`), in order: each record listed, the archive
    /// written for `desc` to read
    DescXml {
        /// Where the made archive goes
        #[arg(long, value_name = "FILE")]
        out: Option<String>,
        paths: Vec<String>,
    },

    /// Build-tree variants, and what each one caps
    Variants { paths: Vec<String> },

    /// DSND phase arrays, and one record's phases
    Sound {
        /// Show this record's phases
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        paths: Vec<String>,
    },

    /// `SMNU` panel layouts: every item, with the rectangles spelled out
    Panels {
        /// Show only this panel
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        /// Archives and `UIData` files, or directories to search for them
        paths: Vec<String>,
    },

    /// The front end's panels from `MAJESTY_HD`: main menu, briefing, summary;
    /// each control's rectangle, picture, font and text
    Menu {
        /// Which dataset: the captures show the base game's panels
        #[arg(long, value_enum, default_value = "base")]
        dataset: MenuDataset,
        /// Show only this panel
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        /// A quest code, such as QU01: its briefing pictures, video and voice
        #[arg(long, value_name = "CODE")]
        quest: Option<String>,
        /// Write every picture as a PAM file in this directory
        #[arg(long, value_name = "DIR")]
        dump: Option<String>,
        /// Also show these pictures, as RECORD:SET[:SLOT] (INse:1023)
        #[arg(long, value_name = "RECORD:SET")]
        #[allow(rustdoc::broken_intra_doc_links, reason = "clap help, not a link")]
        still: Vec<String>,
    },

    /// HD's quest map: the map, the frame, and each quest's place, icons,
    /// difficulty, requirements and video. Reads `MAJESTY_HD`
    QuestMap {
        /// A `UIData_<w>_<h>.dat`; HD's 1920x1200 one by default
        #[arg(long, value_name = "PATH")]
        layout: Option<String>,
        /// Screen width, for where the map opens
        #[arg(long, default_value_t = 1920)]
        width: i32,
        /// Screen height
        #[arg(long, default_value_t = 1200)]
        height: i32,
    },

    /// Every interface picture of `MAJESTY_HD` as an RGBA PNG, with an
    /// index.tsv: screen colours (565), shadow and glow translucent
    UiDump {
        /// Where to write; folders per archive, `tiles/`, `portraits/`
        out: String,
    },

    /// The tileset, its terrain classes, and one blend's slot table
    Terrain {
        /// Show this blend's cases, named like `MapTextureAtoB`
        #[arg(long, value_name = "BLEND")]
        record: Option<String>,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// An .rgs generator chart: its patterns, layer by layer
    Chart {
        /// An archive or directory to resolve references against, for names
        #[arg(long, value_name = "PATH")]
        resolve: Option<String>,
        /// Show only the pattern with this four-character code
        #[arg(long, value_name = "CODE")]
        record: Option<String>,
        paths: Vec<String>,
    },

    /// A .q quest map: its size, and the chart patterns its zones name
    Quest(quest::Args),

    /// BDEP and QDEP: what a building or quest needs, printed as infix
    Deps {
        /// Archives or a directory, for names: buildings from DUNT, quests
        /// from STRT QUES
        #[arg(long, value_name = "PATH")]
        resolve: Option<String>,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// Dump the prototypes in a .bcd's section 3
    Datasec { paths: Vec<String> },

    /// List what a .bcd's section 2 defines, with body sizes
    Codesec { paths: Vec<String> },

    /// Parse every recognised game file and report failures
    CheckData { dirs: Vec<String> },

    /// One line per unit, across every archive given
    Units {
        /// Only units of this subtype
        #[arg(long)]
        kind: Option<Kind>,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// One unit, and everything its ids point at
    Unit {
        /// The DUNT record to resolve
        #[arg(long, value_name = "ID")]
        record: String,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// Where a unit's art paints its shadow: per tile of a set, a PNG of
    /// the tile drawn and of what each pixel is (shadow, colour, dark)
    Shadows {
        /// The DUNT record
        #[arg(long, value_name = "ID")]
        record: String,
        /// The set kind, e.g. active, inactive
        #[arg(long, default_value = "active")]
        set: String,
        /// Directory for the PNGs
        #[arg(long)]
        out: String,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// What the selection panel and ring show for a unit
    Selection {
        /// The DUNT record; omit for the empty panel only
        #[arg(long, value_name = "ID")]
        record: Option<String>,
        /// Archives, or directories to search for them
        paths: Vec<String>,
    },

    /// Dump tokens with spans
    Lex { paths: Vec<String> },

    /// Parse and summarise the syntax tree
    Parse { paths: Vec<String> },

    /// Compile a GPL project to the .bcd Majesty loads, as Gplbcc.exe does
    /// (MakeGPL.bat: `gplbcc -in <project>.gplproj -out <name>.bcd`)
    Compile { project: String, out: String },

    /// Check every $Name resolves, and list builtins
    Resolve { paths: Vec<String> },

    /// Parse .dat prototype files
    Dat { paths: Vec<String> },

    /// Lex and parse every GPL source, and report failures
    CheckGpl { paths: Vec<String> },
}

// One arm per command: a table, not logic.
#[allow(clippy::too_many_lines)]
fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Sniff { paths } => archive::cmd_sniff(&paths),
        Command::Tags { paths } => archive::cmd_tags(&paths),
        Command::List { paths } => archive::cmd_list(&paths),
        Command::Strings { paths } => archive::cmd_strings(&paths),
        Command::Extract { archive, dir } => archive::cmd_extract(&archive, &dir),
        Command::CheckData { dirs } => archive::cmd_check_data(&dirs),

        Command::Tiles { paths } => art::cmd_tiles(&paths),
        Command::Imag { record, paths } => art::cmd_imag(record.as_deref(), &paths),
        Command::Shadows {
            record,
            set,
            out,
            paths,
        } => shadows::cmd_shadows(&record, &set, std::path::Path::new(&out), &paths),

        Command::Desc { record, paths } => descriptor::cmd_desc(record.as_deref(), &paths),
        Command::DescXml { out, paths } => descriptor::cmd_desc_xml(out.as_deref(), &paths),
        Command::Sound { record, paths } => descriptor::cmd_sound(record.as_deref(), &paths),
        Command::Variants { paths } => descriptor::cmd_variants(&paths),

        Command::Datasec { paths } => bytecode::cmd_datasec(&paths),
        Command::Codesec { paths } => bytecode::cmd_codesec(&paths),

        Command::Panels { record, paths } => panel::cmd_panels(record.as_deref(), &paths),
        Command::Menu {
            dataset,
            record,
            quest,
            dump,
            still,
        } => menu::cmd_menu(
            dataset.into(),
            record.as_deref(),
            quest.as_deref(),
            dump.as_deref(),
            &still,
        ),
        Command::QuestMap {
            layout,
            width,
            height,
        } => questmap::cmd_quest_map(layout.as_deref(), (width, height)),
        Command::UiDump { out } => uidump::cmd_ui_dump(std::path::Path::new(&out)),
        Command::Terrain { record, paths } => terrain::cmd_terrain(record.as_deref(), &paths),

        Command::Deps { resolve, paths } => config::cmd_deps(resolve.as_deref(), &paths),

        Command::Chart {
            resolve,
            record,
            paths,
        } => quest::cmd_chart(resolve.as_deref(), record.as_deref(), &paths),

        Command::Quest(args) => quest::cmd_quest(&args),

        Command::Units { kind, paths } => view::cmd_units(kind.map(Into::into), &paths),
        Command::Unit { record, paths } => view::cmd_unit(&record, &paths),
        Command::Selection { record, paths } => view::cmd_selection(record.as_deref(), &paths),

        Command::Lex { paths } => gpl::cmd_lex(&paths),
        Command::Parse { paths } => gpl::cmd_parse(&paths),
        Command::Resolve { paths } => gpl::cmd_resolve(&paths),
        Command::Dat { paths } => gpl::cmd_dat(&paths),
        Command::CheckGpl { paths } => gpl::cmd_check_gpl(&paths),
        Command::Compile { project, out } => gpl::cmd_compile(&project, &out),
    };

    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("regent_cli: {e}");
            ExitCode::FAILURE
        }
    }
}
