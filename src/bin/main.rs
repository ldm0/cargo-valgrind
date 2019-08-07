use cargo_valgrind::{build_target, targets, valgrind, Build, Leak, Target};
use clap::{crate_authors, crate_name, crate_version, App, Arg, ArgMatches};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// The Result type for this application.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The result of the valgrind run.
enum Report {
    /// The analyzed binary contains leaks.
    ContainsErrors,
    /// There was no error detected in the analyzed binary.
    NoErrorDetected,
}

/// Build the command line interface.
///
/// The CLI currently supports the distinction between debug and release builds
/// (selected via the `--release` flag) as well as the selection of the target
/// to execute. Currently binaries, examples and benches are supported.
fn cli<'a, 'b>() -> App<'a, 'b> {
    App::new(crate_name!())
        .about("Cargo subcommand for running valgrind")
        .author(crate_authors!())
        .version(crate_version!())
        .arg(
            Arg::with_name("release")
                .help("Build and run artifacts in release mode, with optimizations")
                .long("release"),
        )
        .arg(
            Arg::with_name("bin")
                .help("Build and run the specified binary")
                .long("bin")
                .takes_value(true)
                .value_name("NAME")
                .conflicts_with_all(&["example", "bench"]),
        )
        .arg(
            Arg::with_name("example")
                .help("Build and run the specified example")
                .long("example")
                .takes_value(true)
                .value_name("NAME")
                .conflicts_with_all(&["bin", "bench"]),
        )
        .arg(
            Arg::with_name("bench")
                .help("Build and run the specified bench")
                .long("bench")
                .takes_value(true)
                .value_name("NAME")
                .conflicts_with_all(&["bin", "example"]),
        )
        .arg(
            Arg::with_name("manifest")
                .help("Path to Cargo.toml")
                .long("manifest-path")
                .takes_value(true)
                .value_name("PATH"),
        )
}

/// Query the build type (debug/release) from the the command line parameters.
fn build_type(parameters: &ArgMatches) -> Build {
    if parameters.is_present("release") {
        Build::Release
    } else {
        Build::Debug
    }
}

/// Query the path to the `Cargo.toml` from the the command line parameters.
///
/// This defaults to the current directory, if the `--manifest-path` parameter
/// is not given.
///
/// # Errors
/// This function fails, if the specified path is not valid.
fn manifest(parameters: &ArgMatches) -> Result<PathBuf> {
    let manifest = parameters
        .value_of("manifest")
        .unwrap_or("Cargo.toml".into());
    let manifest = PathBuf::from(manifest).canonicalize()?;
    Ok(manifest)
}

/// Query the specified `Target`, if any.
fn specified_target(parameters: &ArgMatches) -> Option<Target> {
    parameters
        .value_of("bin")
        .map(|path| Target::Binary(PathBuf::from(path)))
        .or(parameters
            .value_of("example")
            .map(|path| Target::Example(PathBuf::from(path))))
        .or(parameters
            .value_of("bench")
            .map(|path| Target::Benchmark(PathBuf::from(path))))
}

/// Search for the actual binary to analyze.
///
/// This function takes the output of `specified_target()`, as well as the list
/// of all possible targets returned by `targets()`. It searches, if the
/// requested binary exists. If no binary was specified and there is only one
/// target available, that target is used.
///
/// # Errors
/// This function returns an error, if there is no target specified and there
/// are multiple targets to choose from, or if the user specified a non-existing
/// target.
fn find_target(specified: Option<Target>, targets: &[Target]) -> Result<Target> {
    let target = match specified {
        Some(path) => path,
        None if targets.len() == 1 => targets[0].clone(),
        None => Err("Multiple possible targets, please specify more precise")?,
    };
    let target = targets
        .into_iter()
        .find(|&path| path == &target)
        .cloned()
        .ok_or("Could not find selected binary")?;
    Ok(target)
}

/// Display a single `Leak` to the console.
fn display_error(leak: Leak) {
    println!(
        "{:>12} Leaked {} bytes",
        "Error".red().bold(),
        leak.leaked_bytes()
    );
    let mut info = Some("Info".cyan().bold());
    for function in leak.back_trace() {
        println!("{:>12} at {}", info.take().unwrap_or_default(), function);
    }
}

/// Run the specified target inside of valgrind and print the output.
fn analyze_target(target: &Target, manifest: &Path) -> Result<Report> {
    let crate_root = manifest.parent().ok_or("Invalid empty manifest path")?;
    let target_path = target
        .path()
        .strip_prefix(crate_root)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    println!("{:>12} `{}`", "Analyzing".green().bold(), target_path);

    let errors = valgrind(target.path())?;
    if errors.is_empty() {
        Ok(Report::NoErrorDetected)
    } else {
        errors.into_iter().for_each(display_error);
        Ok(Report::ContainsErrors)
    }
}

fn run() -> Result<Report> {
    let cli = cli().get_matches();
    let build = build_type(&cli);
    let target = specified_target(&cli);
    let manifest = manifest(&cli)?;

    let targets = targets(&manifest, build)?;
    let target = find_target(target, &targets)?;
    build_target(&manifest, build, target.clone())?;
    analyze_target(&target, &manifest)
}

fn main() {
    match run() {
        Ok(Report::NoErrorDetected) => {}
        Ok(Report::ContainsErrors) => std::process::exit(1),
        Err(e) => {
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    }
}
