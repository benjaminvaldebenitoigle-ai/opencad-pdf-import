//! PDF vector importer for Open CAD Studio 2026.36.

mod pdf_import;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ocs_plugin_api::host::acadrust::Handle;
use ocs_plugin_api::host::{BuiltinPlugin, HostApi};
use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};
use ocs_plugin_api::ribbon::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.pdf_import",
    name: "PDF a CAD",
    version: env!("CARGO_PKG_VERSION"),
    description: "Importa geometria vectorial de un PDF al dibujo CAD activo",
    api_version: ApiVersion::CURRENT,
    ribbon_order: 45,
    xdata_apps: &[],
    command_prefixes: &["PDFCAD_"],
};

struct PdfCadModule;

impl CadModule for PdfCadModule {
    fn id(&self) -> &'static str {
        "pdf_import"
    }

    fn title(&self) -> &'static str {
        "PDF a CAD"
    }

    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: std::sync::OnceLock<Vec<RibbonGroup>> = std::sync::OnceLock::new();
        GROUPS.get_or_init(|| {
            vec![
                RibbonGroup {
                    title: "Archivo",
                    tools: vec![RibbonItem::LargeTool(ToolDef {
                        id: "PDFCAD_IMPORT",
                        label: "Vista previa",
                        icon: IconKind::Glyph("PDF"),
                        event: ModuleEvent::PluginFileDialog {
                            command: "PDFCAD_IMPORT".to_string(),
                            title: "Seleccionar PDF vectorial".to_string(),
                            filter_name: "Documento PDF".to_string(),
                            extensions: vec!["pdf".to_string()],
                        },
                    })],
                },
                RibbonGroup {
                    title: "Orientacion",
                    tools: vec![
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_ROTATE_LEFT",
                            label: "Girar izquierda",
                            icon: IconKind::Glyph("↶"),
                            event: ModuleEvent::Command("PDFCAD_ROTATE_LEFT".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_ROTATE_RIGHT",
                            label: "Girar derecha",
                            icon: IconKind::Glyph("↷"),
                            event: ModuleEvent::Command("PDFCAD_ROTATE_RIGHT".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_ZOOM_PREVIEW",
                            label: "Ajustar vista",
                            icon: IconKind::Glyph("⌗"),
                            event: ModuleEvent::Command("ZOOM EXTENTS".to_string()),
                        }),
                    ],
                },
                RibbonGroup {
                    title: "Posicion (10 mm)",
                    tools: vec![
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_MOVE_LEFT",
                            label: "Izquierda",
                            icon: IconKind::Glyph("←"),
                            event: ModuleEvent::Command("PDFCAD_MOVE_LEFT".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_MOVE_RIGHT",
                            label: "Derecha",
                            icon: IconKind::Glyph("→"),
                            event: ModuleEvent::Command("PDFCAD_MOVE_RIGHT".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_MOVE_UP",
                            label: "Arriba",
                            icon: IconKind::Glyph("↑"),
                            event: ModuleEvent::Command("PDFCAD_MOVE_UP".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_MOVE_DOWN",
                            label: "Abajo",
                            icon: IconKind::Glyph("↓"),
                            event: ModuleEvent::Command("PDFCAD_MOVE_DOWN".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_CENTER",
                            label: "Posicion original",
                            icon: IconKind::Glyph("⊙"),
                            event: ModuleEvent::Command("PDFCAD_CENTER".to_string()),
                        }),
                    ],
                },
                RibbonGroup {
                    title: "Escala",
                    tools: vec![
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_SCALE_DOWN",
                            label: "Reducir 10%",
                            icon: IconKind::Glyph("−"),
                            event: ModuleEvent::Command("PDFCAD_SCALE_DOWN".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_SCALE_UP",
                            label: "Aumentar 10%",
                            icon: IconKind::Glyph("+"),
                            event: ModuleEvent::Command("PDFCAD_SCALE_UP".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_SCALE_RESET",
                            label: "Escala 100%",
                            icon: IconKind::Glyph("1:1"),
                            event: ModuleEvent::Command("PDFCAD_SCALE_RESET".to_string()),
                        }),
                    ],
                },
                RibbonGroup {
                    title: "Insertar",
                    tools: vec![
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_CONFIRM",
                            label: "Insertar plano",
                            icon: IconKind::Glyph("✓"),
                            event: ModuleEvent::Command("PDFCAD_CONFIRM".to_string()),
                        }),
                        RibbonItem::LargeTool(ToolDef {
                            id: "PDFCAD_CANCEL",
                            label: "Cancelar",
                            icon: IconKind::Glyph("×"),
                            event: ModuleEvent::Command("PDFCAD_CANCEL".to_string()),
                        }),
                    ],
                },
            ]
        })
    }
}

struct PdfCadPlugin;

struct PendingImport {
    result: pdf_import::ImportResult,
    path: PathBuf,
    quarter_turns: u8,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    preview_handles: Vec<Handle>,
    tab_id: u64,
}

static PENDING: OnceLock<Mutex<Option<PendingImport>>> = OnceLock::new();

fn pending() -> &'static Mutex<Option<PendingImport>> {
    PENDING.get_or_init(|| Mutex::new(None))
}

impl BuiltinPlugin for PdfCadPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(PdfCadModule)
    }

    fn dispatch(&self, host: &mut dyn HostApi, command_line: &str) -> bool {
        let (command, argument) = command_line
            .split_once(char::is_whitespace)
            .map_or((command_line, ""), |(command, argument)| {
                (command, argument.trim())
            });
        match command {
            "PDFCAD_IMPORT" => import_preview(host, argument),
            "PDFCAD_ROTATE_LEFT" => rotate_preview(host, 3),
            "PDFCAD_ROTATE_RIGHT" => rotate_preview(host, 1),
            "PDFCAD_MOVE_LEFT" => move_preview(host, -10.0, 0.0),
            "PDFCAD_MOVE_RIGHT" => move_preview(host, 10.0, 0.0),
            "PDFCAD_MOVE_UP" => move_preview(host, 0.0, 10.0),
            "PDFCAD_MOVE_DOWN" => move_preview(host, 0.0, -10.0),
            "PDFCAD_CENTER" => center_preview(host),
            "PDFCAD_SCALE_DOWN" => scale_preview(host, 1.0 / 1.1),
            "PDFCAD_SCALE_UP" => scale_preview(host, 1.1),
            "PDFCAD_SCALE_RESET" => reset_scale(host),
            "PDFCAD_MOVE" => move_preview_exact(host, argument),
            "PDFCAD_SCALE" => scale_preview_exact(host, argument),
            "PDFCAD_CONFIRM" => confirm_import(host),
            "PDFCAD_CANCEL" => cancel_preview(host),
            _ => return false,
        }
        true
    }
}

fn import_preview(host: &mut dyn HostApi, argument: &str) {
    let path_text = argument.trim_matches('"');
    if path_text.is_empty() {
        host.push_error("No se selecciono ningun archivo PDF.");
        return;
    }

    let path = Path::new(path_text);
    host.push_info("Analizando geometria vectorial del PDF...");
    match pdf_import::read_pdf(path) {
        Ok(result) if result.entities.is_empty() => {
            host.push_error(
                    "El PDF no contiene trazados vectoriales importables. Puede ser un documento escaneado; vectoricelo antes de importarlo.",
                );
        }
        Ok(result) => {
            let mut guard = pending()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(previous) = guard.as_mut() {
                if previous.tab_id != host.tab_id() {
                    host.push_error(
                            "Hay una vista previa abierta en otro dibujo. Vuelva a ese dibujo y pulse Cancelar o Insertar plano.",
                        );
                    return;
                }
                remove_preview(host, &mut previous.preview_handles);
            }

            let preview = pdf_import::preview_entities(&result.entities, 0, 1.0, 0.0, 0.0);
            let preview_handles = host.add_entities(preview);
            host.bump_geometry();
            let entity_count = result.entities.len();
            let source_paths = result.source_paths;
            let vertices = result.vertices;
            let pages = result.pages;
            if entity_count > 50_000 {
                host.push_info(
                        "Plano de alta complejidad: la vista previa esta limitada para mantener fluida la aplicacion. La insercion conservara toda la geometria.",
                    );
            }
            if result.text_operators > 0 || result.image_operators > 0 {
                host.push_info(&format!(
                        "Aviso: se omitieron {} operacion(es) de texto y {} imagen(es); esta version importa geometria vectorial.",
                        result.text_operators, result.image_operators
                    ));
            }
            *guard = Some(PendingImport {
                result,
                path: path.to_path_buf(),
                quarter_turns: 0,
                scale: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
                preview_handles,
                tab_id: host.tab_id(),
            });
            host.push_output(&format!(
                    "Vista previa lista: {pages} pagina(s), {vertices} vertices y {entity_count} entidades para {source_paths} trazados. No se elimina ni combina geometria. Gire si es necesario y pulse Insertar plano."
                ));
        }
        Err(error) => host.push_error(&format!("No se pudo importar el PDF: {error}")),
    }
}

fn rotate_preview(host: &mut dyn HostApi, delta: u8) {
    let mut guard = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(item) = guard.as_mut() else {
        host.push_error("Primero pulse Vista previa y seleccione un PDF.");
        return;
    };
    if item.tab_id != host.tab_id() {
        host.push_error("La vista previa pertenece a otro dibujo.");
        return;
    }
    item.quarter_turns = (item.quarter_turns + delta) % 4;
    refresh_preview(host, item);
    host.push_output(&format!(
        "Vista previa girada a {}°.",
        item.quarter_turns as u16 * 90
    ));
}

fn move_preview(host: &mut dyn HostApi, dx: f64, dy: f64) {
    with_pending_preview(host, |host, item| {
        item.offset_x += dx;
        item.offset_y += dy;
        refresh_preview(host, item);
        host.push_output(&format!(
            "Vista previa desplazada: X {:+.2} mm, Y {:+.2} mm.",
            item.offset_x, item.offset_y
        ));
    });
}

fn center_preview(host: &mut dyn HostApi) {
    with_pending_preview(host, |host, item| {
        item.offset_x = 0.0;
        item.offset_y = 0.0;
        refresh_preview(host, item);
        host.push_output("La vista previa volvio a su posicion original.");
    });
}

fn scale_preview(host: &mut dyn HostApi, factor: f64) {
    with_pending_preview(host, |host, item| {
        item.scale = (item.scale * factor).clamp(0.01, 100.0);
        refresh_preview(host, item);
        host.push_output(&format!(
            "Escala de vista previa: {:.2}%.",
            item.scale * 100.0
        ));
    });
}

fn reset_scale(host: &mut dyn HostApi) {
    with_pending_preview(host, |host, item| {
        item.scale = 1.0;
        refresh_preview(host, item);
        host.push_output("Escala de vista previa restablecida a 100%.");
    });
}

fn move_preview_exact(host: &mut dyn HostApi, argument: &str) {
    let Some((x, y)) = argument.split_once(',') else {
        host.push_error("Use PDFCAD_MOVE X,Y; las unidades son milimetros.");
        return;
    };
    let (Ok(x), Ok(y)) = (x.trim().parse::<f64>(), y.trim().parse::<f64>()) else {
        host.push_error("El desplazamiento debe contener dos numeros: PDFCAD_MOVE X,Y.");
        return;
    };
    if !x.is_finite() || !y.is_finite() {
        host.push_error("El desplazamiento debe contener valores finitos.");
        return;
    }
    move_preview(host, x, y);
}

fn scale_preview_exact(host: &mut dyn HostApi, argument: &str) {
    let Ok(scale) = argument.trim().parse::<f64>() else {
        host.push_error("Use PDFCAD_SCALE factor; por ejemplo, PDFCAD_SCALE 0.5.");
        return;
    };
    if !scale.is_finite() || !(0.01..=100.0).contains(&scale) {
        host.push_error("El factor de escala debe estar entre 0.01 y 100.");
        return;
    }
    with_pending_preview(host, |host, item| {
        item.scale = scale;
        refresh_preview(host, item);
        host.push_output(&format!(
            "Escala de vista previa: {:.2}%.",
            item.scale * 100.0
        ));
    });
}

fn with_pending_preview(
    host: &mut dyn HostApi,
    action: impl FnOnce(&mut dyn HostApi, &mut PendingImport),
) {
    let mut guard = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(item) = guard.as_mut() else {
        host.push_error("Primero pulse Vista previa y seleccione un PDF.");
        return;
    };
    if item.tab_id != host.tab_id() {
        host.push_error("La vista previa pertenece a otro dibujo.");
        return;
    }
    action(host, item);
}

fn refresh_preview(host: &mut dyn HostApi, item: &mut PendingImport) {
    remove_preview(host, &mut item.preview_handles);
    item.preview_handles = host.add_entities(pdf_import::preview_entities(
        &item.result.entities,
        item.quarter_turns,
        item.scale,
        item.offset_x,
        item.offset_y,
    ));
    host.bump_geometry();
}

fn confirm_import(host: &mut dyn HostApi) {
    let mut guard = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut item) = guard.take() else {
        host.push_error("Primero pulse Vista previa y seleccione un PDF.");
        return;
    };
    if item.tab_id != host.tab_id() {
        *guard = Some(item);
        host.push_error("La vista previa pertenece a otro dibujo.");
        return;
    }

    remove_preview(host, &mut item.preview_handles);
    host.bump_geometry();
    host.push_undo("Importar PDF");
    let entity_count = item.result.entities.len();
    let pages = item.result.pages;
    let vertices = item.result.vertices;
    let entities = pdf_import::transformed_entities(
        item.result.entities,
        item.quarter_turns,
        item.scale,
        item.offset_x,
        item.offset_y,
    );
    host.add_entities(entities);
    host.bump_geometry();
    host.set_dirty();
    host.push_output(&format!(
        "PDF insertado: {entity_count} entidades, {vertices} vertices, {pages} pagina(s), giro {}°, escala {:.2}%, desplazamiento X {:+.2} mm / Y {:+.2} mm. Archivo: {}. Escala base: 1 punto PDF = {:.6} mm.",
        item.quarter_turns as u16 * 90,
        item.scale * 100.0,
        item.offset_x,
        item.offset_y,
        item.path.display(),
        pdf_import::POINT_TO_MM
    ));
}

fn cancel_preview(host: &mut dyn HostApi) {
    let mut guard = pending()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut item) = guard.take() else {
        host.push_info("No hay una vista previa pendiente.");
        return;
    };
    if item.tab_id != host.tab_id() {
        *guard = Some(item);
        host.push_error("La vista previa pertenece a otro dibujo.");
        return;
    }
    remove_preview(host, &mut item.preview_handles);
    host.bump_geometry();
    host.push_output("Importacion cancelada; la vista previa fue retirada.");
}

fn remove_preview(host: &mut dyn HostApi, handles: &mut Vec<Handle>) {
    for handle in handles.drain(..) {
        host.remove_entity(handle);
    }
}

ocs_plugin_api::export_plugin!(PdfCadPlugin);
