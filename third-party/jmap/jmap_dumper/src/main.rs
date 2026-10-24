use anyhow::{Result, bail};
use clap::{ArgGroup, Parser};
use jmap::Jmap;
use jmap_dumper::{ConfigOverrides, DumpOptions, Input, diag, into_header, structs::Structs};
use std::io::{Cursor, Write};
use std::{collections::BTreeMap, fs::File, io::BufWriter, path::PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None,
        group = ArgGroup::new("input").args(&["pid", "minidump", "jmap", "macho_core"]).required(true))]
struct Cli {
    /// Dump from process ID
    #[arg(long, short, group = "input")]
    pid: Option<i32>,

    /// Dump from minidump
    #[arg(long, short, group = "input")]
    minidump: Option<PathBuf>,

    /// Use existing .jmap dump
    #[arg(long, short, group = "input")]
    jmap: Option<PathBuf>,

    /// Dump from a macOS Mach-O core dump
    #[arg(long, group = "input", value_name = "PATH")]
    macho_core: Option<PathBuf>,

    /// FNamePool address
    #[arg(long, value_parser = parse_hex_u64, value_name = "HEX")]
    fname_pool: Option<u64>,

    /// GUObjectArray address
    #[arg(long, value_parser = parse_hex_u64, value_name = "HEX")]
    guobject_array: Option<u64>,

    /// UE engine version, e.g. 5.4
    #[arg(long, value_parser = parse_engine_version, value_name = "MAJOR.MINOR")]
    engine_version: Option<(u16, u16)>,

    /// Main image load address
    #[arg(long, value_parser = parse_hex_u64, value_name = "HEX")]
    image_base: Option<u64>,

    /// Resolve --fname-pool/--guobject-array as RVA offsets from this module's
    /// load address (minidump input only). Matches on the module basename, e.g.
    /// libUnreal.so. Also defaults --image-base to the module base.
    #[arg(long, value_name = "NAME")]
    module: Option<String>,

    /// Build changelist string
    #[arg(long)]
    build_changelist: Option<String>,

    /// Build has case preserving FNames
    #[arg(long)]
    case_preserving: bool,

    /// Build packs FUObjectItem
    #[arg(long)]
    pack_fuobject_item: bool,

    /// Editor build: sets UE_EDITOR, WITH_EDITOR and WITH_EDITORONLY_DATA
    #[arg(long)]
    editor: bool,

    /// Build has stats enabled (STATS): any non-shipping/test target, editor or not
    #[arg(long)]
    stats: bool,

    /// Build has FUObjectItem Flags+RefCount
    #[arg(long)]
    fuobject_flags_refcount: bool,

    /// Target triple for struct layout, e.g. aarch64-linux-android (defaults to x86_64-pc-windows-msvc)
    #[arg(long, value_parser = jmap_dumper::structs::parse_target_triplet, value_name = "TRIPLE")]
    target: Option<jmap_dumper::structs::TargetTriplet>,

    /// Struct layout info .json (from pdb_dumper)
    #[arg(long, short)]
    struct_info: Option<PathBuf>,

    /// Dump all objects instead of only native (/Script/) objects
    #[arg(long)]
    all: bool,

    /// Dump FName table
    #[arg(long)]
    names: bool,

    /// Verbosity
    #[arg(long, short = 'v', action = clap::ArgAction::Count, conflicts_with = "quiet")]
    verbose: u8,

    /// Suppress warnings and the summary line; only errors are printed
    #[arg(long, short = 'q')]
    quiet: bool,

    /// Skip vtables
    #[arg(long)]
    skip_vtables: bool,

    /// Skip objects
    #[arg(long)]
    skip_objects: bool,

    /// Output format (default: inferred from the output file extension, else jmap)
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    /// Output dump path (.jmap, .jmap.gz, .usmap, .h/.hpp), or `-` for stdout
    #[arg(index = 1)]
    output: PathBuf,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum OutputFormat {
    Jmap,
    JmapGz,
    Usmap,
    Header,
}

fn parse_hex_u64(s: &str) -> Result<u64, String> {
    let trimmed = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(trimmed, 16).map_err(|e| format!("invalid hex u64 {s:?}: {e}"))
}

fn parse_engine_version(s: &str) -> Result<(u16, u16), String> {
    let (maj, min) = s
        .split_once('.')
        .ok_or_else(|| format!("expected MAJOR.MINOR, got {s:?}"))?;
    let maj: u16 = maj.parse().map_err(|e| format!("major: {e}"))?;
    let min: u16 = min.parse().map_err(|e| format!("minor: {e}"))?;
    Ok((maj, min))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    diag::set_verbosity(match (cli.quiet, cli.verbose) {
        (true, _) => diag::Verbosity::Quiet,
        (_, 0) => diag::Verbosity::Normal,
        (_, 1) => diag::Verbosity::Verbose,
        _ => diag::Verbosity::Trace,
    });

    let to_stdout = cli.output.as_os_str() == "-";

    let format = match (cli.format, cli.output.file_name().and_then(|e| e.to_str())) {
        (Some(format), _) => format,
        (None, Some(n)) if n.ends_with(".jmap.gz") => OutputFormat::JmapGz,
        (None, Some(n)) if n.ends_with(".jmap") => OutputFormat::Jmap,
        (None, Some(n)) if n.ends_with(".usmap") => OutputFormat::Usmap,
        (None, Some(n)) if n.ends_with(".h") || n.ends_with(".hpp") => OutputFormat::Header,
        _ => OutputFormat::Jmap,
    };

    let struct_info: Option<Structs> = if let Some(path) = cli.struct_info {
        Some(serde_json::from_slice(&std::fs::read(path)?)?)
    } else {
        None
    };

    let options = DumpOptions {
        all: cli.all,
        names: cli.names,
        skip_vtables: cli.skip_vtables,
        skip_objects: cli.skip_objects,
    };

    let overrides = ConfigOverrides {
        guobject_array: cli.guobject_array,
        fname_pool: cli.fname_pool,
        engine_version: cli.engine_version,
        image_base: cli.image_base,
        build_change_list: cli.build_changelist.clone(),
        build_config: jmap_dumper::structs::BuildConfig {
            case_preserving: cli.case_preserving,
            pack_fuobject_item: cli.pack_fuobject_item,
            stats: cli.stats,
            fuobject_flags_refcount: cli.fuobject_flags_refcount,
            ..if cli.editor {
                jmap_dumper::structs::BuildConfig::editor()
            } else {
                Default::default()
            }
        },
        target_triplet: cli.target,
        module: cli.module.clone(),
    };

    let reflection_data: Jmap = if let Some(path) = cli.jmap {
        let filename = path.file_name().unwrap().to_str().unwrap();
        if filename.ends_with(".jmap.gz") {
            let compressed = std::fs::read(path)?;
            let decoder = flate2::read::GzDecoder::new(Cursor::new(&compressed));
            serde_json::from_reader(decoder)?
        } else if filename.ends_with(".jmap") {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            bail!("Error: Expected .jmap or .jmap.gz file as input");
        }
    } else if let Some(pid) = cli.pid {
        jmap_dumper::dump(Input::Process(pid), overrides, struct_info, options)?
    } else if let Some(path) = cli.minidump {
        jmap_dumper::dump(Input::Dump(path), overrides, struct_info, options)?
    } else if let Some(path) = cli.macho_core {
        jmap_dumper::dump(Input::MachoCore(path), overrides, struct_info, options)?
    } else {
        unreachable!();
    };

    let stdout = std::io::stdout();
    let mut out: BufWriter<Box<dyn Write>> = BufWriter::new(if to_stdout {
        Box::new(stdout.lock())
    } else {
        Box::new(File::create(&cli.output)?)
    });

    match format {
        OutputFormat::Jmap => {
            serde_json::to_writer_pretty(&mut out, &reflection_data)?;
        }
        OutputFormat::JmapGz => {
            let mut e = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
            serde_json::to_writer_pretty(&mut e, &reflection_data)?;
            e.finish()?;
        }
        OutputFormat::Usmap => {
            into_usmap(&reflection_data).write(&mut out)?;
        }
        OutputFormat::Header => {
            out.write_all(into_header(&reflection_data).as_bytes())?;
        }
    }
    out.flush()?;

    if !cli.quiet {
        // keep the data stream on stdout clean when that's where the dump went
        let summary = summary(&reflection_data);
        if to_stdout {
            eprintln!("{summary}");
        } else {
            println!("{summary}");
        }
    }

    Ok(())
}

fn summary(jmap: &Jmap) -> String {
    let mut parts = vec![];
    if let Some(meta) = &jmap.metadata {
        parts.push(format!(
            "UE {}.{}",
            meta.engine_version.major, meta.engine_version.minor
        ));
        if let Some(cl) = &meta.build_change_list {
            parts.push(cl.clone());
        }
    }
    parts.push(format!("{} objects", jmap.objects.len()));
    parts.push(format!("{} vtables", jmap.vtables.len()));
    let warnings = diag::warning_count();
    if warnings > 0 {
        parts.push(format!("{warnings} warnings"));
    }
    parts.join(" ")
}

fn obj_name(path: &str) -> &str {
    path.rsplit(['/', '.', ':']).next().unwrap()
}

fn into_usmap(reflection_data: &Jmap) -> usmap::Usmap {
    let mut enums = vec![];
    let mut structs = vec![];

    for (path, obj) in &reflection_data.objects {
        let struct_ = match &obj {
            jmap::ObjectType::ScriptStruct(obj) => Some(&obj.r#struct),
            jmap::ObjectType::Class(obj) => Some(&obj.r#struct),
            _ => None,
        };
        if let Some(s) = struct_ {
            let mut properties = vec![];
            let mut index = 0;
            for prop in &s.properties {
                properties.push(into_usmap_prop(index, prop));
                index += prop.array_dim;
            }
            structs.push(usmap::Struct {
                name: obj_name(path).to_string(),
                super_struct: s.super_struct.as_ref().map(|s| obj_name(s).to_string()),
                properties,
            });
        } else if let Some(e) = obj.get_enum() {
            let prefix = format!("{}::", obj_name(path));
            let mut entries = BTreeMap::new();
            for (name, value) in &e.names {
                let variant_name = if let Some(variant_name) = name.strip_prefix(&prefix) {
                    variant_name
                } else {
                    assert!(!name.contains("::"), "enum prefix was not stripped");
                    name
                };
                entries.insert(*value, variant_name.to_string());
            }
            enums.push(usmap::Enum {
                name: obj_name(path).to_string(),
                entries,
            });
        }
    }

    usmap::Usmap {
        enums,
        structs,
        cext: None,
        eatr: None,
        envp: None,
        ppth: None,
    }
}

fn into_usmap_prop(index: usize, prop: &jmap::Property) -> usmap::Property {
    usmap::Property {
        name: prop.name.clone(),
        array_dim: prop.array_dim.try_into().unwrap(),
        index: index.try_into().unwrap(),
        inner: into_usmap_prop_inner(&prop.r#type),
    }
}

fn into_usmap_prop_inner(prop: &jmap::PropertyType) -> usmap::PropertyInner {
    use jmap::PropertyType as PT;
    use usmap::PropertyInner as PI;
    match &prop {
        PT::Struct { r#struct } => PI::Struct {
            name: obj_name(r#struct).to_string(),
        },
        PT::Str => PI::Str,
        PT::Name => PI::Name,
        PT::Text => PI::Text,
        // TODO distinguish between sparse/inline?
        PT::MulticastInlineDelegate { .. } => PI::MulticastDelegate,
        PT::MulticastSparseDelegate { .. } => PI::MulticastDelegate,
        PT::MulticastDelegate { .. } => PI::MulticastDelegate,
        PT::Delegate { .. } => PI::Delegate,
        PT::Bool {
            field_size: _,
            byte_offset: _,
            byte_mask: _,
            field_mask: _,
        } => PI::Bool,
        PT::Array { inner } => PI::Array {
            inner: into_usmap_prop_inner(&inner.r#type).into(),
        },
        PT::Enum { container, r#enum } => PI::Enum {
            inner: into_usmap_prop_inner(&container.r#type).into(),
            name: r#enum
                .as_ref()
                .map(|e| obj_name(e))
                .unwrap_or("None")
                .to_string(),
        },
        PT::Map {
            key_prop,
            value_prop,
        } => PI::Map {
            key: into_usmap_prop_inner(&key_prop.r#type).into(),
            value: into_usmap_prop_inner(&value_prop.r#type).into(),
        },
        PT::Set { key_prop } => PI::Set {
            key: into_usmap_prop_inner(&key_prop.r#type).into(),
        },
        PT::Float => PI::Float,
        PT::Double => PI::Double,
        PT::Byte { r#enum } => {
            // usmap special cases ByteProperty to transform into EnumProperty if enum member is populated
            if let Some(e) = r#enum {
                PI::Enum {
                    inner: PI::Byte.into(),
                    name: obj_name(e).to_string(),
                }
            } else {
                PI::Byte
            }
        }
        PT::UInt16 => PI::UInt16,
        PT::UInt32 => PI::UInt32,
        PT::UInt64 => PI::UInt64,
        PT::Int8 => PI::Int8,
        PT::Int16 => PI::Int16,
        PT::Int => PI::Int,
        PT::Int64 => PI::Int64,
        PT::Object { property_class: _ } => PI::Object,
        PT::Class { .. } => PI::Object,
        PT::WeakObject { property_class: _ } => PI::WeakObject,
        PT::SoftObject { property_class: _ } => PI::SoftObject,
        PT::SoftClass { .. } => PI::SoftObject,
        PT::LazyObject { property_class: _ } => PI::LazyObject,
        PT::Interface { interface_class: _ } => PI::Interface,
        PT::FieldPath => PI::FieldPath,
        PT::Optional { inner } => PI::Optional {
            inner: into_usmap_prop_inner(&inner.r#type).into(),
        },
        PT::Utf8Str => PI::Utf8Str,
        PT::AnsiStr => PI::AnsiStr,
    }
}
