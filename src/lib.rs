//! PDF vector importer for Open CAD Studio 2026.36.

mod pdf_import;

use std::path::Path;

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
            vec![RibbonGroup {
                title: "Importar",
                tools: vec![RibbonItem::LargeTool(ToolDef {
                    id: "PDFCAD_IMPORT",
                    label: "Importar PDF",
                    icon: IconKind::Glyph("PDF"),
                    event: ModuleEvent::PluginFileDialog {
                        command: "PDFCAD_IMPORT".to_string(),
                        title: "Seleccionar PDF vectorial".to_string(),
                        filter_name: "Documento PDF".to_string(),
                        extensions: vec!["pdf".to_string()],
                    },
                })],
            }]
        })
    }
}

struct PdfCadPlugin;

impl BuiltinPlugin for PdfCadPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(PdfCadModule)
    }

    fn dispatch(&self, host: &mut dyn HostApi, command_line: &str) -> bool {
        let Some(argument) = command_line.strip_prefix("PDFCAD_IMPORT") else {
            return false;
        };
        let path_text = argument.trim().trim_matches('"');
        if path_text.is_empty() {
            host.push_error("No se selecciono ningun archivo PDF.");
            return true;
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
                let entity_count = result.entities.len();
                host.push_undo("Importar PDF");
                host.add_entities(result.entities);
                host.bump_geometry();
                host.set_dirty();
                host.push_output(&format!(
                    "PDF importado: {entity_count} entidades, {} pagina(s). Escala: 1 punto PDF = {:.6} mm.",
                    result.pages, pdf_import::POINT_TO_MM
                ));
                if result.text_operators > 0 || result.image_operators > 0 {
                    host.push_info(&format!(
                        "Aviso: se omitieron {} operacion(es) de texto y {} imagen(es); esta version importa geometria vectorial.",
                        result.text_operators, result.image_operators
                    ));
                }
            }
            Err(error) => host.push_error(&format!("No se pudo importar el PDF: {error}")),
        }
        true
    }
}

ocs_plugin_api::export_plugin!(PdfCadPlugin);
