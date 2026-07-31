use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{app, board::render_init, task::render};

#[path = "../../boards/nucleo_f401re.rs"]
mod nucleo_f401re;
#[path = "../../apps/nucleo_f401re.rs"]
mod nucleo_f401re_app;

const APP_TEMPLATE: &str = include_str!("../../templates/app.rs.tpl");
const OUTPUT_PATH: &str = "app/generated/src/main.rs";

pub fn generate(repository_root: &Path) -> Result<()> {
    app::validate(&nucleo_f401re_app::APP)?;
    let tasks = render_tasks(repository_root, &nucleo_f401re_app::APP)?;
    let init_spawns = render_init_spawns(&nucleo_f401re_app::APP);
    let board = render_init(
        &nucleo_f401re::BOARD,
        !nucleo_f401re_app::APP.tasks.is_empty(),
    )?;
    let application = substitute(APP_TEMPLATE, &tasks, &init_spawns, &board)?;
    syn::parse_file(&application).context("parse complete generated RTIC application")?;

    let destination = repository_root.join(OUTPUT_PATH);
    let parent = destination
        .parent()
        .expect("generated main path has a parent directory");
    fs::create_dir_all(parent)
        .with_context(|| format!("create generated source directory {}", parent.display()))?;
    fs::write(&destination, application)
        .with_context(|| format!("write generated RTIC app {}", destination.display()))?;
    println!("Generated RTIC app: {}", destination.display());
    Ok(())
}

fn render_tasks(repository_root: &Path, app: &app::AppDeclaration) -> Result<String> {
    app.tasks
        .iter()
        .map(|task| render(task, &read_task_body(repository_root, task.id)?))
        .collect::<Result<Vec<_>>>()
        .map(|tasks| tasks.join("\n\n"))
}

fn read_task_body(repository_root: &Path, task_id: &str) -> Result<String> {
    if task_id.is_empty()
        || !task_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        bail!("task body ID `{task_id}` is not a safe filename");
    }
    let path = repository_root
        .join("tasks/task-bodies")
        .join(format!("{task_id}.rs"));
    fs::read_to_string(&path).with_context(|| format!("read task body {}", path.display()))
}

fn render_init_spawns(app: &app::AppDeclaration) -> String {
    app.init
        .spawns
        .iter()
        .map(|task| {
            format!(
                "{}::spawn().expect(\"init must spawn declared task {}\");",
                task.id, task.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn substitute(
    template: &str,
    tasks: &str,
    init_spawns: &str,
    board: &crate::board::RenderedBoardInit,
) -> Result<String> {
    let replacements = [
        ("{{RTIC_IMPORTS}}", board.imports.clone(), 4),
        (
            "{{MONOTONIC_DECLARATION}}",
            board.monotonic_declaration.clone(),
            4,
        ),
        ("{{LOCAL_STRUCT}}", board.local_struct.clone(), 4),
        ("{{LOCAL_VALUE}}", board.local_value.clone(), 8),
        ("{{BOARD_INIT}}", board.initialization.clone(), 8),
        ("{{INIT_SPAWNS}}", init_spawns.to_owned(), 8),
        ("{{TASKS}}", tasks.to_owned(), 4),
    ];
    let mut rendered = template.to_owned();
    for (marker, value, indentation) in replacements {
        if rendered.matches(marker).count() != 1 {
            bail!("RTIC app template must contain exactly one {marker} marker");
        }
        rendered = rendered.replace(marker, &indent(&value, indentation));
    }
    if board.initialization.is_empty() && init_spawns.trim().is_empty() {
        rendered = rendered.replace(
            "fn init(cx: init::Context) -> (Shared, Local) {\n        \n",
            "fn init(_cx: init::Context) -> (Shared, Local) {\n",
        );
    }
    let mut normalized = rendered
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    while normalized.contains("\n\n\n") {
        normalized = normalized.replace("\n\n\n", "\n\n");
    }
    if board.initialization.is_empty() && init_spawns.trim().is_empty() {
        normalized = normalized.replace(
            "fn init(_cx: init::Context) -> (Shared, Local) {\n\n",
            "fn init(_cx: init::Context) -> (Shared, Local) {\n",
        );
    }
    if tasks.trim().is_empty() {
        normalized = normalized.replace("\n\n}", "\n}");
    }
    normalized.push('\n');
    Ok(normalized)
}

fn indent(source: &str, spaces: usize) -> String {
    source.replace('\n', &format!("\n{}", " ".repeat(spaces)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_application_omits_tasks_and_init_spawns() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let empty_app = app::AppDeclaration::EMPTY;
        let tasks = render_tasks(root, &empty_app).unwrap();
        let spawns = render_init_spawns(&empty_app);
        let board = render_init(&nucleo_f401re::BOARD, false).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();

        assert!(!application.contains("::spawn()"));
        assert!(!application.contains("#[task("));
        assert!(syn::parse_file(&application).is_ok());
    }
}
