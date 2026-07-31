use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{
    app, backend, board,
    resolve::{self, ResolvedTask},
    task::{TaskDeclaration, render},
};

#[path = "../../targets/nucleo_f401re/src/board.rs"]
mod nucleo_f401re;
#[path = "../../targets/nucleo_f401re/src/app_composition.rs"]
mod nucleo_f401re_app;

const APP_TEMPLATE: &str = include_str!("../../templates/app.rs.tpl");
const OUTPUT_PATH: &str = "targets/nucleo_f401re/src/main.rs";

pub fn generate(repository_root: &Path) -> Result<()> {
    app::validate(&nucleo_f401re_app::APP)?;
    board::validate(&nucleo_f401re::BOARD)?;
    backend::validate(&nucleo_f401re::BOARD)?;
    let resolved = resolve::resolve(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP)?;
    let tasks = render_tasks(repository_root, &resolved.tasks)?;
    let init_spawns = render_init_spawns(&resolved.init_spawned_tasks);
    let board = backend::render(&nucleo_f401re::BOARD, &resolved)?;
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

fn render_tasks(repository_root: &Path, tasks: &[ResolvedTask<'_>]) -> Result<String> {
    tasks
        .iter()
        .map(|task| {
            let resolved_locals = task
                .local_resources
                .iter()
                .map(|resource| resource.id())
                .collect::<Vec<_>>();
            let resolved_shared = task
                .shared_resources
                .iter()
                .map(|resource| resource.id())
                .collect::<Vec<_>>();
            if resolved_locals != task.declaration.local_resources
                || resolved_shared != task.declaration.shared_resources
            {
                bail!(
                    "resolved resources for task `{}` do not match its declaration",
                    task.declaration.id
                );
            }
            render(
                task.declaration,
                &read_task_body(repository_root, task.declaration.id)?,
            )
        })
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

fn render_init_spawns(tasks: &[&TaskDeclaration]) -> String {
    tasks
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
    board: &crate::backend::RenderedBoardInit,
) -> Result<String> {
    let replacements = [
        ("{{RTIC_IMPORTS}}", board.imports.clone(), 4),
        (
            "{{MONOTONIC_DECLARATION}}",
            board.monotonic_declaration.clone(),
            4,
        ),
        ("{{SHARED_STRUCT}}", board.shared_struct.clone(), 4),
        ("{{SHARED_VALUE}}", board.shared_value.clone(), 8),
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
    use crate::board::PhysicalPin;

    #[test]
    fn nucleo_board_declares_pa5_as_typed_physical_data() {
        assert_eq!(
            nucleo_f401re::BOARD.hardware[0].pin(),
            PhysicalPin::new("PA5")
        );
    }

    #[test]
    fn empty_application_omits_tasks_and_init_spawns() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let empty_app = app::AppDeclaration::EMPTY;
        app::validate(&empty_app).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        backend::validate(&nucleo_f401re::BOARD).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &empty_app).unwrap();
        let tasks = render_tasks(root, &resolved.tasks).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        let board = backend::render(&nucleo_f401re::BOARD, &resolved).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();

        assert!(!application.contains("::spawn()"));
        assert!(!application.contains("#[task("));
        assert!(syn::parse_file(&application).is_ok());
    }

    #[test]
    fn selected_init_spawn_is_rendered() {
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        backend::validate(&nucleo_f401re::BOARD).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);

        assert!(spawns.contains("blink_led::spawn()"));
    }

    #[test]
    fn blink_includes_one_shot_report_task_with_count_argument() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        backend::validate(&nucleo_f401re::BOARD).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let tasks = render_tasks(root, &resolved.tasks).unwrap();
        let init_spawns = render_init_spawns(&resolved.init_spawned_tasks);

        assert!(tasks.contains("async fn report_blink(cx: report_blink::Context, count: u32)"));
        assert!(tasks.contains("report_blink::spawn(blink_count)"));
        assert!(tasks.contains("defmt::info!(\"Blink {}\", count)"));
        assert!(!init_spawns.contains("report_blink::spawn"));
    }

    #[test]
    fn button_exti_task_and_blink_enable_state_are_rendered() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        app::validate(&nucleo_f401re_app::APP).unwrap();
        board::validate(&nucleo_f401re::BOARD).unwrap();
        backend::validate(&nucleo_f401re::BOARD).unwrap();
        let resolved = resolve::resolve(&nucleo_f401re::BOARD, &nucleo_f401re_app::APP).unwrap();
        let tasks = render_tasks(root, &resolved.tasks).unwrap();
        let spawns = render_init_spawns(&resolved.init_spawned_tasks);
        let board = backend::render(&nucleo_f401re::BOARD, &resolved).unwrap();
        let application = substitute(APP_TEMPLATE, &tasks, &spawns, &board).unwrap();

        assert!(application.contains("binds = EXTI15_10"));
        assert!(application.contains("user_button: PC13<Input>"));
        assert!(application.contains("blink_enabled: bool"));
        assert!(application.contains("clear_interrupt_pending_bit"));
        assert!(application.contains("blink_enabled.lock"));
    }
}
