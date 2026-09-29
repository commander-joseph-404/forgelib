use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command};

#[derive(Debug, Parser)]
#[command(name = "forgelib", 
    about ="A minimal Foundry library cache manager",
    version)]

struct Cli {
    #[command(subcommand)]
    cmd :Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Clone a library into the local cache (skipped if already cached).
    Fetch{
        /// HTTPS Github URL of the repository (e.g. https://github.com/owner/repo)
        repo_url :String,
        /// Short identifier used as the cache key (e.g. openzeppelin-contracts)
        name :String,
        /// Git tag or branch to check out (e.g. v5.0.2)
        version :String
    },

    /// Copy (or Symlink ) a cached Library into ./lib and update the remappings.txt
    Use{
        /// Cached key used when fetching ( must match exactly)
        name :String,
        /// Git tag or branch to check out (e.g. v5.0.2)
        version :String,
        /// Create a symlink Instead of Copying (Unix Only)
        #[arg(long)]
        link :bool
    },

    /// List every cached library and its versions
    List,

    /// Removes a cached library version (or the entire library entry if the version not provided)
    Remove {
        /// Cached key to be removed
        name :String,
        /// Specific version to remove (omit if you want to remove all the versions)
        version :Option<String>
    }
}

// Cache Directory

fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not locate the OS cache directory")?;
    Ok(base.join("forgelib"))
}

// User Input Validation

/// Validates a `name` and `version` passed by the user 
/// Allowed: ASCII alphanumerics plus `-`, `.`, `_` .
/// Rejected: empty, contains `/` or `\`, contains `..`, starts with `.` or `-`.
fn validate_component(s :&str, field_name: &str) -> Result<()> {
    anyhow::ensure!(!s.is_empty(), "{field_name} cannot be empty.");
    anyhow::ensure!(!s.contains(" "), "{field_name} cannot contain ' ' (empty space)");
    anyhow::ensure!(!s.contains("/"), "{field_name} cannot contain '/' (path separator) ");
    anyhow::ensure!(!s.contains("\\"), "{field_name} cannot contain '\\' (path separator) ");
    anyhow::ensure!(!s.contains(".."), "{field_name} cannot contain '..' (path traversal) ");
    anyhow::ensure!(!s.starts_with("."), "{field_name} cannot starts with '.' (hidden/relative path) ");
    anyhow::ensure!(!s.starts_with("-"), "{field_name} cannot starts with '-' (looks like CLI flag) ");
    anyhow::ensure!(s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
                    "{field_name} cannot contain invalid characters \
                    (only ASCII letters, digits, '-' , '_' , '.') are allowed");


    Ok(())
}

/// Validates a repository URL.
///
///Rules:
/// - Must begin with `https://github.com/`
/// - Must not contain spaces
/// - Must not contain query strings (`?`) or fragments (`#`)
/// - Must have a owner and repo path after `github.com/` (i.e. owner/repo at minimum)
fn validate_repo_url (url: &str) -> Result<()> {
    anyhow::ensure!(
        !url.starts_with("-"),
        "repo_url must not start with '-' "
    );
    anyhow::ensure!(
        !url.contains(" "),
        "repo_url must not contain  spaces"
    );
    anyhow::ensure!(
        url.starts_with("https://github.com/"),
        "repo_url must be an HTTPS URL from github.com \
         (e.g. https://github.com/owner/repo)" 
    );
    anyhow::ensure!(
        !url.contains("?"),
        "repo_url must not contain ('?')  a query string"
    );

    anyhow::ensure!(
        !url.contains("#"),
        "repo_url  must not contain  ('#') a URL Fragment "
    );

    // Ensure there is a real path component after " https://github.com/"
    let path_part =  url.strip_prefix("https://github.com/")
                        .unwrap_or("");

    let segments :Vec<&str> = path_part
                            .split("/")
                            .filter(|s| !s.is_empty())
                            .collect();

    anyhow::ensure!(
        segments.len() >= 2, 
        "repo_url must include an owner and a repository name \
        (e.g. https://github.com/owner/repo)"
);

Ok(())
}

//SubCommand Handlers

fn cmd_fetch (repo_url :String, name :String, version :String) -> Result<()>{
    validate_component(&name, "name")?;
    validate_component(&version, "version")?;
    validate_repo_url(&repo_url)?;

    let dest = cache_dir()?.join(&name).join(&version);
    
    if dest.exists() {
        println!("{name}@{version} is already cached at {}", dest.display());
        return Ok(())
    }

    // Ensure that the Parent Directory exists before cloning into dest..

    let parent = dest.parent()
                    .context("computed destination path unexpectedly has no parent")?;
    
    std::fs::create_dir_all(parent)
        .with_context(|| format!( "failed to create cache directory {}", parent.display()))?;
    
    // Resolve Git from the PATH. Using Command::new with plain argument list
    //This helps to avoids shell injection.
    let status = Command::new("git")
                .args(["clone", "--depth", "1", "--branch", &version, &repo_url])
                .arg(&dest)
                .status()
                .context("failed to spawn `git` - is it installed and on your PATH")?;

    anyhow::ensure!(status.success(), "git clone exited with non-zero code");

    std::fs::remove_dir_all(dest.join(".git"))
    .context("failed to remove cloned repository's .git directory")?;
    
    println!("cached {name}@{version} -> {}", dest.display());
    
    Ok(())
}

fn cmd_use(name :String, version :String, link :bool) -> Result<()> {
    validate_component(&name, "name")?;
    validate_component(&version, "version")?;

    let src = cache_dir()?.join(&name).join(&version);
    anyhow::ensure!(
        src.exists(),
        "{name}@{version} is not cached - run `forgelib fetch ` first"
    );

    let lib_dir = PathBuf::from("lib");

    let dest = lib_dir.join(&name);

    std::fs::create_dir_all(&lib_dir)
        .context("failed to create ./lib directory")?;

    if link {
        install_symlink(&src, &dest)?;
    }else {
        install_copy(&src, &dest)?;
    }

    append_remapping(&name, &dest)?;

    println!("installed {name}@{version} into ./lib and updated remappings.txt");

    Ok(())
    
}

fn cmd_list() -> Result<()> {
    let dir = cache_dir()?;

    if !dir.exists() {
        println!("Cache is empty. Run  `forgelib fetch` to add a library.");
        return Ok(());
    }

    let mut found = false;

    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read cache directory {}", dir.display()))?
        {
            let entry = entry.context("failed to read cache directory entry")?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let lib_name = entry.file_name();

            let lib_name = lib_name.to_string_lossy();
            
            let mut versions :Vec<String> = Vec::new();
            for ver_entry in std::fs::read_dir(entry.path())
                .with_context(|| format!("failed to read versions for {lib_name}"))?
                {
                    let ver_entry = ver_entry.context("failed to read version entry")?;
                    versions.push(ver_entry.file_name().to_string_lossy().into_owned());
                }

                versions.sort();

                println!("  {lib_name}");
                for v in &versions {
                    println!("      --{v}");
                }
                found = true;
        }

    if !found {
            println!("Cache is empty. Run `forgelib fetch` to add a library."); 
        }

    Ok(())
}

fn cmd_remove (name :String, version :Option<String>) -> Result<()> {
    validate_component(&name, "name")?;
    let base = cache_dir()?.join(&name);

    anyhow::ensure!(
        base.exists(),
        "no cached entry found for '{name}' "
    );

    match version {
        Some(ver) => {

            validate_component(&ver, "version")?;
            let target = base.join(&ver);

            anyhow::ensure!(
                target.exists(),
                "no cached version '{ver}' found for '{name}'"
            );

            // - Confirmation Prompt Before Removing The Cached Library Version
            println!();
            println!("  Library : {name}");
            println!("  Version : {ver}");
            println!("  Path : {}", target.display());
            println!();
            if !confirm("This will permanently remove the Library Version above. Do you want to continue?")?{
                println!("Aborted, No Library version removed");
                return Ok(());
            }
            std::fs::remove_dir_all(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
            println!("removed {name}@{ver} from the cache");
        }
        None => {

            // - Confirmation Prompt Before Removing The Cached Library Version
            // - Collect all the Cached Version for The Library
            // So the user can see what they are about to remove before actually remove it
            let mut versions :Vec<String> = std::fs::read_dir(&base)
                                                        .with_context(|| format!("failed to read versions for {name}"))?
                                                        .filter_map(|e| e.ok())
                                                        .map(|e| e.file_name().to_string_lossy().into_owned())
                                                        .collect();

            versions.sort();

            println!();
            println!("  Library         : {name}");
            println!("  Path         : {}", base.display());

            if versions.is_empty() {
                println!("  Version         : (none)");
            }else{
                println!("  Total of {} Versions Found   :", versions.len());

                for v in &versions {
                    println!("                      --{v}");
                }
            }


            if !confirm("This will permanently remove All The Version above For this Library. Do you want to continue?")?{
                println!("Aborted, Nothing was removed");
                return Ok(());
            }
            std::fs::remove_dir_all(&base)
                .with_context(|| format!("failed to remove {}", base.display()))?;

            println!("removed all cached versions of {name}");
        }
    }
    Ok(())

}


/// Prints Questions Followed by [Y/N], reads one line from stdin,
/// Returns True only if the user typed y or Yes or YES or yes or Y or yEs , it is not case-insensitive.
/// Pressing Enter alone (empty input) is treated as **No**  (the safe default)

fn confirm(question :&str) -> Result<bool> {
    print!("  {question} [Y/N]:");

    // Flush so the prompt appears before we block on stdin.

    std::io::stdout()
                .flush()
                .context("failed to flush stdout before reading confirmations")?;

    let mut input = String::new();

    std::io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation from stdin")?;

    Ok(matches!(input.trim().to_lowercase().as_str(), "yes" | "y"))
}

// Install Helpers

/// Creates a symlink  `src` ` ->  `dest . Unix-only.
///Returns an error (and does not continue) if the operation fails.
#[cfg(unix)]
fn install_symlink(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() || dest.symlink_metadata().is_ok() {
        bail!(
            "destination {} already exists; remove first",
            dest.display()
        );
    }

    std::os::unix::fs::symlink(src, dest).with_context(|| {
        format!("failed to create symlink {} -> {}",
        dest.display(), src.display()
    )
    })?;


    Ok(())
}

/// Stub for non-Unix platforms: `--link` is not supported.
#[cfg(not(unix))]
fn install_symlink(_src: &Path, _dest: &Path) -> Result<()> {
    bail!(" `--link` is only supported on Unix systems; omit the flag to copy instead")
}

/// Recursively copies src into dest.
fn install_copy(src: &Path, dest: &Path) ->  Result<()> {
    let mut opts = fs_extra::dir::CopyOptions::new();
    opts.copy_inside = true;
    fs_extra::dir::copy(src, dest, &opts)
        .with_context(|| format!("failed to copy {} -> {}", src.display(), dest.display()))?;
    println!("  copied: {} -> {}", src.display(), dest.display());

    Ok(())
}


/// Detects the most likely Solidity source root inside a freshly installed
/// library directory.
///
/// Check order (first match wins):
///   1. `src/`       – forge-std, solmate, most modern libs
///   2. `contracts/` – OpenZeppelin, ERC721A
///   3. `""` (root)  – fallback: remapping points at the lib root itself
fn detect_src_dir(lib_path: &Path) -> &'static str {
    if lib_path.join("src").is_dir() {
        return "src/";
    }
    if lib_path.join("contracts").is_dir() {
        return "contracts/";
    }
    ""  // fallback: remap directly to lib root
}

/// Appends a remapping entry for a name to remappings.txt,
/// **only if an identical line does not already exists**.

fn append_remapping(name: &str, lib_path: &Path) ->  Result<()> {
    let src_dir = detect_src_dir(lib_path);
    let remap_line = format!("{name}/=lib/{name}/{src_dir}");
    let path = PathBuf::from("remappings.txt");

    // Read existing lines (if file exists) and check for duplicates.

    if path.exists() {
        let file = std::fs::File::open(&path)
            .context("failed to open remappings.txt for reading")?;

        let already_present = std::io::BufReader::new(file)
            .lines()
            .any(|l| l.map(|l| l.trim() == remap_line).unwrap_or(false));

        if already_present {
            println!("  remappings.txt already contains '{remap_line}' -- skipped");
            return Ok(());
        }
    }
        let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .context("failed to open remappings.txt for writing")?;
        
       writeln!(f, "{remap_line}").context("failed to write to remappings.txt")?;
        println!("  remappings.txt <- '{remap_line}' "); 
    
    Ok(())
}

//Entry Point

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Fetch {repo_url, name, version} => cmd_fetch(repo_url, name, version),
        Cmd::Use {name, version, link} => cmd_use(name, version, link),
        Cmd::List => cmd_list(),
        Cmd::Remove {name, version} => cmd_remove(name, version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----Validate Component

    #[test]
    fn rejects_path_traversal() {
        let result = validate_component(".../../etc", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain '/' (path separator)"));
    }

    #[test]
    fn rejects_traversal() {
        let result = validate_component("a...b", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain '..' (path traversal)"));
    }
    #[test]
    fn rejects_leading_dash_flag_injection() {
        let result = validate_component("--upload-pack", "version");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot starts with '-' (looks like CLI flag)"));
    }

    #[test]
    fn rejects_leading_dot() {
        let result = validate_component(".hidden", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot starts with '.' (hidden/relative path)"));
    }
    #[test]
    fn rejects_space() {
        let result = validate_component("open  book", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain ' ' (empty space)"));
    }

    #[test]
    fn rejects_slash() {
        let result = validate_component("owner/repo", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain '/' (path separator)"));
    }
    #[test]
    fn rejects_backlash() {
        let result = validate_component("dog\\cat", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain '\\' (path separator)"));
    }
    #[test]
    fn rejects_empty() {
        let result = validate_component("", "version");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot be empty"));
    }

    #[test]
    fn rejects_invalid_char_at() {
        let result = validate_component("lib@v1", "name");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot contain invalid characters"));
    }

    #[test]
    fn accepts_normal_tag() {
        assert!(validate_component("v.0.5.24", "version").is_ok());
    }

    #[test]
    fn accepts_name_with_hyphens_and_underscores() {
        assert!(validate_component("openzeppelin-contracts_v5", "name").is_ok());
    }

    // --- validate_repo_url ---

    #[test]
    fn rejects_plain_http() {
        let result = validate_repo_url("http://github.com/owner/repo");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must be an HTTPS URL from github.com"));
    }

    #[test]
    fn rejects_url_with_space() {
        let result = validate_repo_url("https://github.com/   book");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must not contain  spaces"));
    }

    #[test]
    fn rejects_url_with_query_string() {
        let result = validate_repo_url("https://github.com/owner/repo?");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must not contain ('?')  a query string"));
    }

    #[test]
    fn rejects_url_with_fragment() {
         let result = validate_repo_url("https://github.com/owner/repo#readme");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url  must not contain  ('#') a URL Fragment"));
    }
    

    #[test]
    fn rejects_url_missing_repo_segment() {
        let result = validate_repo_url("https://github.com/owner");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must include an owner and a repository name"));
    }
   
    #[test]
    fn rejects_url_that_starts_with_hyphen() {
        let result = validate_repo_url("-https://github.com/OpenZeppelin/openzeppelin-contracts");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must not start with '-' "));
    }

    #[test]
    fn rejects_bare_github_root() {
        let result = validate_repo_url("https://github.com/");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("repo_url must include an owner and a repository name"));
    }

    #[test]
    fn accept_valid_github_url(){
        assert!(validate_repo_url("https://github.com/OpenZeppelin/openzeppelin-contracts").is_ok());
    }

    #[test]
    fn accepts_valid_url_with_trailing_slash () {
        assert!(validate_repo_url("https://github.com/OpenZeppelin/openzeppelin-contracts/").is_ok());
    }

     // --- detect_src_dir ---

    fn temp_lib(label: &str, layout: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forgelib-test-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for sub in layout {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        dir
    }

    #[test]
    fn src_dir_preferred_over_contracts() {
        let dir = temp_lib("src-pref", &["src", "contracts"]);
        assert_eq!(detect_src_dir(&dir), "src/");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn contracts_dir_used_when_no_src() {
        let dir = temp_lib("contracts-only", &["contracts"]);
        assert_eq!(detect_src_dir(&dir), "contracts/");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_known_layout_falls_back_to_root() {
        let dir = temp_lib("root-fallback", &["lib"]);
        assert_eq!(detect_src_dir(&dir), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
