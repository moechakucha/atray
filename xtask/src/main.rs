use std::{env, fs, io::Cursor, path::Path, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use xshell::{Shell, cmd};

fn main() -> Result<()> {
    let args = Args::parse()?;
    let sh = Shell::new()?;
    let root = workspace_root();

    sh.change_dir(&root);

    let metadata = Metadata::load(&sh, &root)?;

    match args.task {
        Task::Bundle => {
            let app = build_app(&sh, &root, &metadata, &args)?;
            println!("{}", app.display());
        }
        Task::Dmg => {
            let app = build_app(&sh, &root, &metadata, &args)?;
            let dmg = build_dmg(&sh, &metadata, &args, &app)?;
            println!("{}", dmg.display());
        }
        Task::Msi => {
            let msi = build_msi(&sh, &root, &metadata, &args)?;
            println!("{}", msi.display());
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Bundle,
    Dmg,
    Msi,
}

#[derive(Debug)]
struct Args {
    task: Task,
    target: Option<String>,
    release: bool,
    sign: bool,
    build: bool,
    out: PathBuf,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut task = None;
        let mut target = None;
        let mut release = true;
        let mut sign = true;
        let mut build = true;
        let mut out = PathBuf::from("dist");

        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "bundle" => task = Some(Task::Bundle),
                "dmg" => task = Some(Task::Dmg),
                "msi" => task = Some(Task::Msi),
                "--target" => target = Some(args.next().context("--target needs a triple")?),
                "--debug" => release = false,
                "--no-sign" => sign = false,
                "--no-build" => build = false,
                "--out" => out = PathBuf::from(args.next().context("--out needs a path")?),
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => bail!("unknown argument: {other}"),
            }
        }

        Ok(Self {
            task: task.context("expected `bundle`, `dmg` or `msi`")?,
            target,
            release,
            sign,
            build,
            out,
        })
    }

    fn profile(&self) -> &'static str {
        if self.release { "release" } else { "debug" }
    }

    fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn macos_target(&self) -> Result<Option<&str>> {
        let Some(target) = self.target.as_deref() else {
            if env::consts::OS != "macos" {
                bail!(
                    "no --target given and this host is not macOS; \
                     pass e.g. --target aarch64-apple-darwin"
                );
            }

            return Ok(None);
        };

        if !target.contains("apple-darwin") {
            bail!("{target} is not a macOS target (expected *-apple-darwin)");
        }

        Ok(Some(target))
    }

    fn windows_target(&self) -> Result<&str> {
        let Some(target) = self.target.as_deref() else {
            bail!("no --target given; pass e.g. --target x86_64-pc-windows-gnu");
        };

        if !target.contains("windows") {
            bail!("{target} is not a Windows target (expected *-pc-windows-*)");
        }

        Ok(target)
    }

    fn arch(&self) -> String {
        match self.target.as_deref() {
            Some(target) => target.split('-').next().unwrap_or(target).to_owned(),
            None => env::consts::ARCH.to_owned(),
        }
    }
}

fn print_help() {
    println!(
        "cargo xtask bundle [--target <triple>] [--debug] [--no-sign] [--out <dir>]\n\
         cargo xtask dmg    [--target <triple>] [--debug] [--no-sign] [--out <dir>]\n\
         cargo xtask msi    [--target <triple>] [--debug] [--no-build] [--out <dir>]"
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is not at the workspace root")
        .to_path_buf()
}

#[derive(Deserialize)]
struct CargoMetadata {
    target_directory: PathBuf,
    packages: Vec<Package>,
}

impl CargoMetadata {
    fn load(sh: &Shell) -> Result<Self> {
        let json = cmd!(sh, "cargo metadata --no-deps --format-version 1").read()?;

        serde_json::from_str(&json).context("failed to parse `cargo metadata` output")
    }
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    manifest_path: PathBuf,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BundleMeta {
    name: Option<String>,
    identifier: Option<String>,
    icon: Vec<String>,
    resources: Vec<String>,
    category: Option<String>,
    copyright: Option<String>,
    osx_minimum_system_version: Option<String>,
    osx_info_plist_exts: Vec<String>,
    osx_url_schemes: Vec<String>,
}

struct Metadata {
    target_directory: PathBuf,
    name: String,
    version: String,
    binary: String,
    bundle: BundleMeta,
}

impl Metadata {
    fn load(sh: &Shell, root: &Path) -> Result<Self> {
        let cargo = CargoMetadata::load(sh)?;

        let package = cargo
            .packages
            .iter()
            .find(|package| {
                package
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.get("bundle").is_some())
            })
            .or_else(|| {
                cargo
                    .packages
                    .iter()
                    .find(|package| package.manifest_path.parent() == Some(root))
            })
            .context("no package with [package.metadata.bundle] found")?;

        let bundle = package
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("bundle"))
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .context("failed to read [package.metadata.bundle]")?
            .unwrap_or_default();

        let binary = package
            .targets
            .iter()
            .find(|target| target.kind.iter().any(|kind| kind == "bin"))
            .context("no binary target found")?
            .name
            .clone();

        Ok(Self {
            target_directory: cargo.target_directory,
            name: package.name.clone(),
            version: package.version.clone(),
            binary,
            bundle,
        })
    }

    fn bundle_name(&self) -> &str {
        self.bundle.name.as_deref().unwrap_or(&self.name)
    }

    fn identifier(&self) -> &str {
        self.bundle.identifier.as_deref().unwrap_or(&self.name)
    }
}

fn build_app(sh: &Shell, root: &Path, metadata: &Metadata, args: &Args) -> Result<PathBuf> {
    let target = args.macos_target()?;

    build_binary(sh, args, target, &metadata.binary)?;

    let binary = binary_path(args, metadata)?;
    let app = args.out.join(format!("{}.app", metadata.bundle_name()));

    if app.exists() {
        fs::remove_dir_all(&app).with_context(|| format!("failed to clear {app:?}"))?;
    }

    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");

    fs::create_dir_all(&macos)?;
    fs::create_dir_all(&resources)?;

    let executable = macos.join(&metadata.binary);
    fs::copy(&binary, &executable)
        .with_context(|| format!("failed to copy {} into the bundle", binary.display()))?;
    make_executable(&executable)?;

    let icon = copy_icon(root, &resources, &metadata)?;

    for resource in &metadata.bundle.resources {
        let src = root.join(resource);
        let relative = src.strip_prefix(root).unwrap_or(&src);
        let dst = resources.join(relative);

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(&src, &dst)
            .with_context(|| format!("failed to copy resource {}", src.display()))?;
    }

    let plist = info_plist(root, &metadata, icon.as_deref())?;
    fs::write(contents.join("Info.plist"), plist)?;

    if args.sign {
        sign_app(&app)?;
    }

    Ok(app)
}

fn build_binary(sh: &Shell, args: &Args, target: Option<&str>, binary: &str) -> Result<()> {
    let mut cargo = sh.cmd("cargo").arg("build");

    if args.release {
        cargo = cargo.arg("--release");
    }

    if let Some(target) = target {
        cargo = cargo.arg("--target").arg(target);
    }

    cargo = cargo.arg("--bin").arg(binary);

    cargo.run().context("cargo build failed")?;

    Ok(())
}

fn binary_path(args: &Args, metadata: &Metadata) -> Result<PathBuf> {
    let mut dir = metadata.target_directory.clone();

    if let Some(target) = args.target.as_deref() {
        dir.push(target);
    }

    dir.push(args.profile());

    let name = if args
        .target()
        .is_some_and(|target| target.contains("windows"))
    {
        format!("{}.exe", metadata.binary)
    } else {
        metadata.binary.clone()
    };

    let binary = dir.join(name);

    if !binary.is_file() {
        bail!("expected a built binary at {}", binary.display());
    }

    Ok(binary)
}

fn copy_icon(root: &Path, resources: &Path, metadata: &Metadata) -> Result<Option<String>> {
    let icon = metadata
        .bundle
        .icon
        .iter()
        .find(|icon| icon.ends_with(".icns"))
        .or_else(|| metadata.bundle.icon.first());

    let Some(icon) = icon else {
        return Ok(None);
    };

    let src = root.join(icon);
    let name = src
        .file_name()
        .context("icon has no file name")?
        .to_string_lossy()
        .into_owned();

    fs::copy(&src, resources.join(&name))
        .with_context(|| format!("failed to copy icon {}", src.display()))?;

    Ok(Some(name))
}

fn info_plist(root: &Path, metadata: &Metadata, icon: Option<&str>) -> Result<String> {
    let identifier = metadata
        .bundle
        .identifier
        .as_deref()
        .context("[package.metadata.bundle] identifier is required for a macOS bundle")?;

    let name = metadata.bundle_name();
    let mut plist = String::new();

    plist.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple Computer//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n",
    );

    entry(&mut plist, "CFBundleDevelopmentRegion", "English");
    entry(&mut plist, "CFBundleDisplayName", name);
    entry(&mut plist, "CFBundleExecutable", &metadata.binary);
    entry(&mut plist, "CFBundleIdentifier", identifier);
    entry(&mut plist, "CFBundleInfoDictionaryVersion", "6.0");
    entry(&mut plist, "CFBundleName", name);
    entry(&mut plist, "CFBundlePackageType", "APPL");
    entry(&mut plist, "CFBundleShortVersionString", &metadata.version);
    entry(&mut plist, "CFBundleVersion", &metadata.version);

    if let Some(icon) = icon {
        entry(&mut plist, "CFBundleIconFile", icon);
    }

    if let Some(category) = metadata.bundle.category.as_deref().and_then(category_type) {
        entry(&mut plist, "LSApplicationCategoryType", &category);
    }

    if let Some(version) = &metadata.bundle.osx_minimum_system_version {
        entry(&mut plist, "LSMinimumSystemVersion", version);
    }

    if let Some(copyright) = &metadata.bundle.copyright {
        entry(&mut plist, "NSHumanReadableCopyright", copyright);
    }

    plist.push_str("  <key>NSHighResolutionCapable</key>\n  <true/>\n");

    if !metadata.bundle.osx_url_schemes.is_empty() {
        url_types(&mut plist, name, &metadata.bundle.osx_url_schemes);
    }

    for extension in &metadata.bundle.osx_info_plist_exts {
        let path = root.join(extension);
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        plist.push_str(&contents);
    }

    plist.push_str("</dict>\n</plist>\n");

    Ok(plist)
}

fn entry(plist: &mut String, key: &str, value: &str) {
    plist.push_str(&format!(
        "  <key>{key}</key>\n  <string>{}</string>\n",
        escape(value)
    ));
}

fn url_types(plist: &mut String, name: &str, schemes: &[String]) {
    plist.push_str("  <key>CFBundleURLTypes</key>\n  <array>\n    <dict>\n");
    plist.push_str(&format!(
        "      <key>CFBundleURLName</key>\n      <string>{}</string>\n",
        escape(name)
    ));
    plist.push_str(
        "      <key>CFBundleTypeRole</key>\n      <string>Viewer</string>\n      \
         <key>CFBundleURLSchemes</key>\n      <array>\n",
    );

    for scheme in schemes {
        plist.push_str(&format!("        <string>{}</string>\n", escape(scheme)));
    }

    plist.push_str("      </array>\n    </dict>\n  </array>\n");
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn category_type(category: &str) -> Option<String> {
    const PREFIX: &str = "public.app-category.";

    if category.starts_with(PREFIX) {
        return Some(category.to_owned());
    }

    let normalized: String = category
        .to_lowercase()
        .chars()
        .filter(|c| *c != ' ' && *c != '-')
        .collect();

    CATEGORIES
        .iter()
        .find(|(name, _)| *name == normalized)
        .map(|(_, suffix)| format!("{PREFIX}{suffix}"))
}

const CATEGORIES: &[(&str, &str)] = &[
    ("business", "business"),
    ("developertool", "developer-tools"),
    ("education", "education"),
    ("entertainment", "entertainment"),
    ("finance", "finance"),
    ("game", "games"),
    ("actiongame", "action-games"),
    ("adventuregame", "adventure-games"),
    ("arcadegame", "arcade-games"),
    ("boardgame", "board-games"),
    ("cardgame", "card-games"),
    ("casinogame", "casino-games"),
    ("dicegame", "dice-games"),
    ("educationalgame", "educational-games"),
    ("familygame", "family-games"),
    ("kidsgame", "kids-games"),
    ("musicgame", "music-games"),
    ("puzzlegame", "puzzle-games"),
    ("racinggame", "racing-games"),
    ("roleplayinggame", "role-playing-games"),
    ("simulationgame", "simulation-games"),
    ("sportsgame", "sports-games"),
    ("strategygame", "strategy-games"),
    ("triviagame", "trivia-games"),
    ("wordgame", "word-games"),
    ("graphicsanddesign", "graphics-design"),
    ("healthcareandfitness", "healthcare-fitness"),
    ("lifestyle", "lifestyle"),
    ("medical", "medical"),
    ("music", "music"),
    ("news", "news"),
    ("photography", "photography"),
    ("productivity", "productivity"),
    ("reference", "reference"),
    ("socialnetworking", "social-networking"),
    ("sports", "sports"),
    ("travel", "travel"),
    ("utility", "utilities"),
    ("video", "video"),
    ("weather", "weather"),
];

fn sign_app(app: &Path) -> Result<()> {
    let settings = apple_codesign::SigningSettings::default();
    let signer = apple_codesign::UnifiedSigner::new(settings);
    signer
        .sign_path_in_place(app)
        .with_context(|| format!("failed to sign {}", app.display()))?;

    Ok(())
}

fn build_dmg(sh: &Shell, metadata: &Metadata, args: &Args, app: &Path) -> Result<PathBuf> {
    let name = metadata.bundle_name();

    fs::create_dir_all(&args.out)?;

    let dmg = args.out.join(format!("{name}-{}.dmg", args.arch()));

    if dmg.exists() {
        fs::remove_file(&dmg).with_context(|| format!("failed to clear {dmg:?}"))?;
    }

    let temp = tempfile::tempdir().context("failed to create a staging directory")?;
    let staging = temp.path().join("staging");
    fs::create_dir_all(&staging)?;

    let staged_app = staging.join(app.file_name().context("app bundle has no file name")?);

    copy_dir(app, &staged_app)?;
    symlink(Path::new("/Applications"), &staging.join("Applications"))?;

    let image = temp.path().join("image.hfs");

    genisoimage(sh, name, &image, &staging)?;
    dmg_tool(sh, &image, &dmg)?;

    Ok(dmg)
}

fn genisoimage(sh: &Shell, volume: &str, image: &Path, source: &Path) -> Result<()> {
    let tool = tool_from_env("GENISOIMAGE", "genisoimage");

    sh.cmd(&tool)
        .arg("-V")
        .arg(volume)
        .args(["-D", "-R", "-apple", "-no-pad"])
        .arg("-o")
        .arg(image)
        .arg(source)
        .run()
        .with_context(|| {
            format!(
                "failed to run {} (install cdrkit or set GENISOIMAGE)",
                tool.display()
            )
        })?;

    Ok(())
}

fn dmg_tool(sh: &Shell, image: &Path, dmg: &Path) -> Result<()> {
    let tool = tool_from_env("DMG_TOOL", "dmg");

    sh.cmd(&tool).arg(image).arg(dmg).run().with_context(|| {
        format!(
            "failed to run {} (build libdmg-hfsplus or set DMG_TOOL)",
            tool.display()
        )
    })?;

    Ok(())
}

fn tool_from_env(variable: &str, default: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn build_msi(sh: &Shell, root: &Path, metadata: &Metadata, args: &Args) -> Result<PathBuf> {
    let target = args.windows_target()?;

    if args.build {
        build_binary(sh, args, Some(target), &metadata.binary)?;
    } else {
        binary_path(args, metadata)?;
    }

    fs::create_dir_all(&args.out)?;

    let binary = binary_path(args, metadata)?;
    let name = metadata.bundle_name();
    let msi = args.out.join(format!("{name}-{}.msi", args.arch()));
    let manifest = args.out.join(format!("{name}-{}.wxs", args.arch()));

    if msi.exists() {
        fs::remove_file(&msi).with_context(|| format!("failed to clear {msi:?}"))?;
    }

    let icon = windows_icon(root, metadata, &args.out)?;
    let source = wxs_source(root, metadata, &binary, args.arch(), icon.as_deref());

    fs::write(&manifest, source).with_context(|| format!("failed to write {manifest:?}"))?;

    wixl(sh, &manifest, &msi, wixl_arch(target))?;

    if let Some(platform) = wixl_platform(target) {
        msibuild(sh, &msi, metadata, platform)?;
    }

    Ok(msi)
}

fn windows_icon(root: &Path, metadata: &Metadata, out: &Path) -> Result<Option<PathBuf>> {
    let Some(icon) = metadata
        .bundle
        .icon
        .iter()
        .find(|icon| icon.ends_with(".png"))
    else {
        return Ok(None);
    };

    let source = root.join(icon);
    let image = image::open(&source)
        .with_context(|| format!("failed to open {}", source.display()))?
        .into_rgba8();
    let image = image::imageops::resize(
        &image,
        WINDOWS_ICON_SIZE,
        WINDOWS_ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );

    let mut png = Vec::new();

    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .context("failed to encode the icon as PNG")?;

    let icon = out.join(format!("{}.ico", metadata.bundle_name()));
    let mut bytes = Vec::with_capacity(png.len() + 22);

    bytes.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&(png.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&22u32.to_le_bytes());
    bytes.extend_from_slice(&png);

    fs::write(&icon, bytes).with_context(|| format!("failed to write {icon:?}"))?;

    Ok(Some(icon))
}

fn wxs_source(
    root: &Path,
    metadata: &Metadata,
    binary: &Path,
    arch: String,
    icon: Option<&Path>,
) -> String {
    let identifier = metadata.identifier();
    let name = escape(metadata.bundle_name());
    let version = escape(&metadata.version);
    let manufacturer = escape(&manufacturer(identifier));
    let upgrade = stable_guid(&format!("{identifier} upgrade"));
    let product = stable_guid(&format!("{identifier} {version} {arch} product"));
    let package = stable_guid(&format!("{identifier} {version} {arch} package"));
    let component = stable_guid(&format!("{identifier} {arch} executable component"));
    let file = binary
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}.exe", metadata.binary));
    let source = escape(&relative(root, binary));

    let mut wxs = String::new();

    wxs.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    wxs.push_str("<Wix xmlns=\"http://schemas.microsoft.com/wix/2006/wi\">\n");
    wxs.push_str(&format!(
        "  <Product Id=\"{product}\" Name=\"{name}\" Language=\"1033\" Version=\"{version}\" \
         Manufacturer=\"{manufacturer}\" UpgradeCode=\"{upgrade}\">\n"
    ));
    wxs.push_str(&format!(
        "    <Package Id=\"{package}\" Keywords=\"Installer\" Description=\"{name} {version}\" \
         Manufacturer=\"{manufacturer}\" InstallerVersion=\"500\" Compressed=\"yes\" />\n"
    ));
    wxs.push_str(&format!(
        "    <MajorUpgrade DowngradeErrorMessage=\"A newer version of {name} is already installed.\" />\n"
    ));
    wxs.push_str("    <Property Id=\"ALLUSERS\" Value=\"1\" />\n");
    wxs.push_str("    <Property Id=\"ARPNOMODIFY\" Value=\"1\" />\n");
    wxs.push_str(&format!(
        "    <Media Id=\"1\" Cabinet=\"{identifier}.cab\" EmbedCab=\"yes\" />\n"
    ));

    if let Some(icon) = icon {
        wxs.push_str(&format!(
            "    <Icon Id=\"AtrayIcon\" SourceFile=\"{}\" />\n",
            escape(&relative(root, icon))
        ));
        wxs.push_str("    <Property Id=\"ARPPRODUCTICON\" Value=\"AtrayIcon\" />\n");
    }

    wxs.push_str("    <Directory Id=\"TARGETDIR\" Name=\"SourceDir\">\n");
    wxs.push_str("      <Directory Id=\"ProgramFiles64Folder\">\n");
    wxs.push_str(&format!(
        "        <Directory Id=\"INSTALLFOLDER\" Name=\"{name}\">\n"
    ));
    wxs.push_str(&format!(
        "          <Component Id=\"AtrayExecutable\" Guid=\"{component}\" Win64=\"yes\">\n"
    ));
    wxs.push_str(&format!(
        "            <File Id=\"AtrayExecutableFile\" Name=\"{file}\" Source=\"{source}\" \
         KeyPath=\"yes\" />\n"
    ));
    wxs.push_str(&format!(
        "            <Shortcut Id=\"AtrayStartMenu\" Directory=\"ProgramMenuFolder\" Name=\"{name}\" \
         WorkingDirectory=\"INSTALLFOLDER\" Advertise=\"no\" />\n"
    ));
    wxs.push_str("          </Component>\n");
    wxs.push_str("        </Directory>\n");
    wxs.push_str("      </Directory>\n");
    wxs.push_str("      <Directory Id=\"ProgramMenuFolder\" />\n");
    wxs.push_str("    </Directory>\n");
    wxs.push_str(&format!(
        "    <Feature Id=\"Complete\" Title=\"{name}\" Level=\"1\">\n"
    ));
    wxs.push_str("      <ComponentRef Id=\"AtrayExecutable\" />\n");
    wxs.push_str("    </Feature>\n");
    wxs.push_str("  </Product>\n");
    wxs.push_str("</Wix>\n");

    wxs
}

fn wixl(sh: &Shell, manifest: &Path, msi: &Path, arch: &str) -> Result<()> {
    let tool = tool_from_env("WIXL", "wixl");

    sh.cmd(&tool)
        .arg("--arch")
        .arg(arch)
        .arg("-o")
        .arg(msi)
        .arg(manifest)
        .run()
        .with_context(|| {
            format!(
                "failed to run {} (install msitools or set WIXL)",
                tool.display()
            )
        })?;

    Ok(())
}

fn wixl_arch(target: &str) -> &'static str {
    match target.split('-').next() {
        Some("i586" | "i686") => "x86",
        _ => "x64",
    }
}

fn wixl_platform(target: &str) -> Option<&'static str> {
    match target.split('-').next() {
        Some("aarch64") => Some("Arm64"),
        _ => None,
    }
}

fn msibuild(sh: &Shell, msi: &Path, metadata: &Metadata, platform: &str) -> Result<()> {
    let tool = tool_from_env("MSIBUILD", "msibuild");

    sh.cmd(&tool)
        .arg(msi)
        .arg("-s")
        .arg(metadata.bundle_name())
        .arg(manufacturer(metadata.identifier()))
        .arg(format!("{platform};1033"))
        .run()
        .with_context(|| {
            format!(
                "failed to run {} (install msitools or set MSIBUILD)",
                tool.display()
            )
        })?;

    Ok(())
}

fn stable_guid(seed: &str) -> String {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    let mut hash = OFFSET;

    for byte in seed.bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }

    let bytes = hash.to_be_bytes();

    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn manufacturer(identifier: &str) -> String {
    match identifier.rsplit_once('.') {
        Some((prefix, _)) => prefix.to_owned(),
        None => identifier.to_owned(),
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

const WINDOWS_ICON_SIZE: u32 = 256;

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }

    Ok(())
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

fn symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)?;
    }

    Ok(())
}
