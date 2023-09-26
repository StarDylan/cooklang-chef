use std::{fs, path::Path};

use anstream::{print, println};
use anyhow::{bail, Context as _, Result};
use camino::Utf8PathBuf;
use clap::Args;
use cooklang::Extensions;

use crate::{
    config::{
        config_file_path, global_file_path, global_store, store_at_path, Config, GlobalConfig,
        DEFAULT_CONFIG_FILE, GLOBAL_CONFIG_FILE,
    },
    Context, COOK_DIR, UTF8_PATH_PANIC,
};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Run the basic interactive config setup
    #[arg(long, exclusive = true)]
    setup: bool,
    /// Display (or --edit) global config file
    #[arg(long)]
    global: bool,
    /// Display (or --edit) the global default config file
    ///
    /// This config is used when no config.toml is defined in the current directory
    /// under the .cooklang dir.
    #[arg(long)]
    default: bool,
}

pub fn run_setup(config: &Config, global_config: &GlobalConfig) -> Result<()> {
    use inquire::{Confirm, Text};
    use owo_colors::OwoColorize;

    let mut config = config.clone();

    let chef = "chef".green().italic().to_string();
    let cooklang = "cooklang".yellow().to_string();

    println!("Welcome to {chef}!");
    println!();
    println!(
        "{chef} uses an extended version of {cooklang}. You can learn \
        more here:\n\thttps://github.com/cooklang/cooklang-rs/blob/main/extensions.md"
    );
    println!();
    config.extensions = extensions_prompt(config.extensions)?;

    println!();
    for line in textwrap::wrap(
        &format!(
            "Chef uses collections to store recipes. A collection is just a \
            directory where a `{COOK_DIR}` dir exists. If you set up a default\
            collection, you can run {chef} anywhere and access your recipes. \
            Otherwise, you will have to provide a path or be in a collection."
        ),
        textwrap::termwidth().min(80),
    ) {
        println!("{line}");
    }
    println!();

    let initial_path = global_config.default_collection.clone().unwrap_or_else(|| {
        let dirs = directories::UserDirs::new();
        let parent = if let Some(d) = &dirs {
            d.document_dir().unwrap_or(d.home_dir())
        } else {
            Path::new(".")
        };
        let dp = parent.join("Recipes");
        Utf8PathBuf::from_path_buf(dp).expect(UTF8_PATH_PANIC)
    });
    let path = Text::new("Default collection path:")
        .with_initial_value(initial_path.as_str())
        .with_help_message("Leave empty or press ESC for none")
        .prompt_skippable()?
        .filter(|s| !s.is_empty())
        .map(Utf8PathBuf::from);

    if let Some(path) = &path {
        if path.exists() {
            if !path.is_dir() {
                bail!("The path is not a dir: {path}");
            }
            if !path.join(COOK_DIR).is_dir() && path.read_dir()?.any(|_| true) {
                bail!("The path is not empty: {path}");
            }
        } else {
            let create = Confirm::new("The directory does not exist. Do you want to create it?")
                .with_default(true)
                .prompt()?;
            if create {
                fs::create_dir_all(path).context("Failed to create recipes directory")?;
            }
        }

        if path.exists() {
            let config_path = config_file_path(path);
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            store_at_path(&config_path, &config)?;
        }
    }

    println!("Default collection created and configured");

    println!();
    for line in textwrap::wrap(
        &format!(
            "If you use {chef} outside the new collection by using the \
            `--path` arg or creating another one, {chef} will use the \
            default configuration."
        ),
        textwrap::termwidth().min(80),
    ) {
        println!("{line}");
    }
    println!();

    let set_default = Confirm::new("Do you want this to be the default config as well?")
        .with_default(true)
        .prompt()?;
    if set_default {
        global_store(DEFAULT_CONFIG_FILE, &config)?;
    }

    global_store(
        GLOBAL_CONFIG_FILE,
        GlobalConfig {
            default_collection: path,
            ..global_config.clone()
        },
    )?;

    println!();
    println!("{chef} is configured!");

    Ok(())
}

fn extensions_prompt(enabled: Extensions) -> Result<Extensions> {
    use std::ops::BitOr;

    let items = Extensions::all()
        .iter_names()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    let enabled = Extensions::all()
        .iter()
        .enumerate()
        .filter_map(|(index, flag)| enabled.contains(flag).then_some(index))
        .collect::<Vec<_>>();

    let selected = inquire::MultiSelect::new("Enable extensions", items)
        .with_default(&enabled)
        .prompt()?;

    Ok(selected
        .iter()
        .map(|n| Extensions::from_name(n).unwrap())
        .fold(Extensions::empty(), Extensions::bitor))
}

pub fn run(ctx: &Context, args: ConfigArgs) -> Result<()> {
    if args.setup {
        run_setup(&ctx.config, &ctx.global_config)?;
        return Ok(());
    }

    if args.global {
        display_global(ctx)
    } else {
        display_regular(ctx)
    }
}

fn display_regular(ctx: &Context) -> Result<()> {
    use owo_colors::OwoColorize;

    println!("Recipes path: {}", ctx.base_path.yellow());

    let mut config_path = config_file_path(&ctx.base_path);
    if !config_path.is_file() {
        print!("{}", "No config at: ".dimmed());
        println!("{}", config_path.dimmed().bright_red());
        config_path = global_file_path(DEFAULT_CONFIG_FILE)?;
    }
    println!("Config: {}", config_path.yellow());

    let fence = "+++".dimmed();
    println!("{fence}");
    let c = toml::to_string_pretty(&ctx.config)?;
    println!("{}", c.trim());
    println!("{fence}");

    for file in ctx
        .config
        .units(&ctx.base_path)
        .iter()
        .chain(ctx.config.aisle(&ctx.base_path).iter())
    {
        print!("{file} {} ", "--".dimmed());
        if file.is_file() {
            println!("{}", "found".green().bold());
        } else {
            println!("{}", "not found".red().bold());
        }
    }

    Ok(())
}

fn display_global(ctx: &Context) -> Result<()> {
    use owo_colors::OwoColorize;

    let global_path = global_file_path(GLOBAL_CONFIG_FILE)?;
    println!("Global config: {}", global_path.yellow());

    let fence = "+++".dimmed();
    println!("{fence}");
    let c = toml::to_string_pretty(&ctx.global_config)?;
    println!("{}", c.trim());
    println!("{fence}");
    Ok(())
}
