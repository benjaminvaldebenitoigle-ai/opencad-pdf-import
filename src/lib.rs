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

            let preview = pdf_import::preview_entities(&result.entities, 0);
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
    remove_preview(host, &mut item.preview_handles);
    item.quarter_turns = (item.quarter_turns + delta) % 4;
    item.preview_handles = host.add_entities(pdf_import::preview_entities(
        &item.result.entities,
        item.quarter_turns,
    ));
    host.bump_geometry();
    host.push_output(&format!(
        "Vista previa girada a {}°.",
        item.quarter_turns as u16 * 90
    ));
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
    let entities = pdf_import::rotated_entities(item.result.entities, item.quarter_turns);
    host.add_entities(entities);
    host.bump_geometry();
    host.set_dirty();
    host.push_output(&format!(
        "PDF insertado: {entity_count} entidades, {vertices} vertices, {pages} pagina(s), giro {}°. Archivo: {}. Escala: 1 punto PDF = {:.6} mm.",
        item.quarter_turns as u16 * 90,
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
