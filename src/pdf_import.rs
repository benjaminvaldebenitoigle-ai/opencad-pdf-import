use std::path::Path;

use lopdf::{content::Content, Dictionary, Document, Object, Stream};
use ocs_plugin_api::host::acadrust::{EntityType, Line, LwPolyline, Vector2, Vector3};

pub const POINT_TO_MM: f64 = 25.4 / 72.0;
const PAGE_GAP_MM: f64 = 10.0;
const CURVE_STEPS: usize = 12;
const PREVIEW_ENTITY_LIMIT: usize = 240;

#[derive(Debug)]
pub struct ImportResult {
    pub entities: Vec<EntityType>,
    pub pages: usize,
    pub source_paths: usize,
    pub vertices: usize,
    pub text_operators: usize,
    pub image_operators: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn from_pdf(v: &[f64]) -> Self {
        Self {
            a: v[0],
            b: v[1],
            c: v[2],
            d: v[3],
            e: v[4],
            f: v[5],
        }
    }

    fn then(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    fn apply(self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.e,
            y: self.b * p.x + self.d * p.y + self.f,
        }
    }
}

#[derive(Default)]
struct Subpath {
    points: Vec<Point>,
    closed: bool,
}

struct PageState {
    ctm: Matrix,
    stack: Vec<Matrix>,
    paths: Vec<Subpath>,
    current: Option<usize>,
    painted_paths: Vec<Subpath>,
    text_operators: usize,
    image_operators: usize,
}

impl PageState {
    fn new(page_offset_x: f64) -> Self {
        Self {
            ctm: Matrix {
                a: POINT_TO_MM,
                d: POINT_TO_MM,
                e: page_offset_x,
                ..Matrix::IDENTITY
            },
            stack: Vec::new(),
            paths: Vec::new(),
            current: None,
            painted_paths: Vec::new(),
            text_operators: 0,
            image_operators: 0,
        }
    }

    fn move_to(&mut self, point: Point) {
        let point = self.ctm.apply(point);
        self.paths.push(Subpath {
            points: vec![point],
            closed: false,
        });
        self.current = Some(self.paths.len() - 1);
    }

    fn line_to(&mut self, point: Point) {
        let point = self.ctm.apply(point);
        if let Some(index) = self.current {
            self.paths[index].points.push(point);
        } else {
            self.move_to(point);
        }
    }

    fn current_point(&self) -> Option<Point> {
        self.current
            .and_then(|i| self.paths.get(i)?.points.last().copied())
    }

    fn curve_to(&mut self, c1: Point, c2: Point, end: Point) {
        let Some(start) = self.current_point() else {
            self.move_to(end);
            return;
        };
        let c1 = self.ctm.apply(c1);
        let c2 = self.ctm.apply(c2);
        let end = self.ctm.apply(end);
        if let Some(index) = self.current {
            for step in 1..=CURVE_STEPS {
                let t = step as f64 / CURVE_STEPS as f64;
                let mt = 1.0 - t;
                self.paths[index].points.push(Point {
                    x: mt.powi(3) * start.x
                        + 3.0 * mt.powi(2) * t * c1.x
                        + 3.0 * mt * t.powi(2) * c2.x
                        + t.powi(3) * end.x,
                    y: mt.powi(3) * start.y
                        + 3.0 * mt.powi(2) * t * c1.y
                        + 3.0 * mt * t.powi(2) * c2.y
                        + t.powi(3) * end.y,
                });
            }
        }
    }

    fn close(&mut self) {
        if let Some(index) = self.current {
            self.paths[index].closed = true;
        }
    }

    fn rectangle(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.move_to(Point { x, y });
        self.line_to(Point { x: x + width, y });
        self.line_to(Point {
            x: x + width,
            y: y + height,
        });
        self.line_to(Point { x, y: y + height });
        self.close();
    }

    fn paint(&mut self, close: bool) {
        if close {
            self.close();
        }
        self.painted_paths.append(&mut self.paths);
        self.current = None;
    }

    fn discard_path(&mut self) {
        self.paths.clear();
        self.current = None;
    }
}

pub fn read_pdf(path: &Path) -> Result<ImportResult, String> {
    let document = Document::load(path).map_err(|e| e.to_string())?;
    if document.is_encrypted() {
        return Err("el PDF esta cifrado o protegido con contrasena".to_string());
    }

    let pages = document.get_pages();
    let mut all_entities = Vec::new();
    let mut source_paths = 0;
    let mut vertices = 0;
    let mut text_operators = 0;
    let mut image_operators = 0;
    let mut page_offset_x = 0.0;

    for page_id in pages.values() {
        let bytes = document
            .get_page_content(*page_id)
            .map_err(|e| e.to_string())?;
        let content = Content::decode(&bytes).map_err(|e| e.to_string())?;
        let mut state = PageState::new(page_offset_x);
        let (direct_resources, resource_ids) = document
            .get_page_resources(*page_id)
            .map_err(|e| e.to_string())?;
        let mut resources = Vec::new();
        if let Some(resource) = direct_resources {
            resources.push(resource);
        }
        for resource_id in resource_ids {
            if let Ok(resource) = document.get_dictionary(resource_id) {
                if !resources.iter().any(|known| std::ptr::eq(*known, resource)) {
                    resources.push(resource);
                }
            }
        }
        process_operations(&document, &content, &resources, &mut state, 0)?;
        state.paint(false);

        source_paths += state.painted_paths.len();
        let page_entities = paths_to_entities(state.painted_paths);
        vertices += page_entities.iter().map(entity_vertex_count).sum::<usize>();

        let page_width = page_width_points(&document, *page_id).unwrap_or(612.0) * POINT_TO_MM;
        page_offset_x += page_width + PAGE_GAP_MM;
        text_operators += state.text_operators;
        image_operators += state.image_operators;
        all_entities.extend(page_entities);
    }

    Ok(ImportResult {
        entities: all_entities,
        pages: pages.len(),
        source_paths,
        vertices,
        text_operators,
        image_operators,
    })
}

fn paths_to_entities(paths: Vec<Subpath>) -> Vec<EntityType> {
    paths
        .into_iter()
        .filter_map(|path| {
            if path.points.len() < 2 {
                return None;
            }
            if !path.closed && path.points.len() == 2 {
                let start = path.points[0];
                let end = path.points[1];
                return Some(EntityType::Line(Line::from_points(
                    Vector3::new(start.x, start.y, 0.0),
                    Vector3::new(end.x, end.y, 0.0),
                )));
            }
            let mut polyline = LwPolyline::from_points(
                path.points
                    .into_iter()
                    .map(|p| Vector2::new(p.x, p.y))
                    .collect(),
            );
            polyline.is_closed = path.closed;
            Some(EntityType::LwPolyline(polyline))
        })
        .collect()
}

pub fn preview_entities(entities: &[EntityType], quarter_turns: u8) -> Vec<EntityType> {
    if entities.is_empty() {
        return Vec::new();
    }
    let half = PREVIEW_ENTITY_LIMIT / 2;
    let mut ranked: Vec<_> = entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (index, entity_span_squared(entity)))
        .collect();
    ranked.sort_unstable_by(|left, right| right.1.total_cmp(&left.1));
    let mut indices: Vec<_> = ranked.into_iter().take(half).map(|item| item.0).collect();
    let step = entities.len().div_ceil(half).max(1);
    indices.extend((0..entities.len()).step_by(step).take(half));
    indices.sort_unstable();
    indices.dedup();
    let mut sampled: Vec<_> = indices
        .into_iter()
        .map(|index| entities[index].clone())
        .collect();
    if let Some((min, max)) = entity_bounds(entities) {
        rotate_entities_around(&mut sampled, min, max, quarter_turns);
    }
    sampled
}

fn entity_span_squared(entity: &EntityType) -> f64 {
    match entity {
        EntityType::Line(line) => {
            let dx = line.end.x - line.start.x;
            let dy = line.end.y - line.start.y;
            dx * dx + dy * dy
        }
        EntityType::LwPolyline(polyline) => {
            let mut min = Vector2::new(f64::INFINITY, f64::INFINITY);
            let mut max = Vector2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
            for vertex in &polyline.vertices {
                min.x = min.x.min(vertex.location.x);
                min.y = min.y.min(vertex.location.y);
                max.x = max.x.max(vertex.location.x);
                max.y = max.y.max(vertex.location.y);
            }
            let dx = max.x - min.x;
            let dy = max.y - min.y;
            dx * dx + dy * dy
        }
        _ => 0.0,
    }
}

pub fn rotated_entities(mut entities: Vec<EntityType>, quarter_turns: u8) -> Vec<EntityType> {
    let Some((min, max)) = entity_bounds(&entities) else {
        return entities;
    };
    rotate_entities_around(&mut entities, min, max, quarter_turns);
    entities
}

fn rotate_entities_around(
    entities: &mut [EntityType],
    min: Vector2,
    max: Vector2,
    quarter_turns: u8,
) {
    let center = Vector2::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
    for entity in entities {
        match entity {
            EntityType::Line(line) => {
                rotate_xy(&mut line.start.x, &mut line.start.y, center, quarter_turns);
                rotate_xy(&mut line.end.x, &mut line.end.y, center, quarter_turns);
            }
            EntityType::LwPolyline(polyline) => {
                for vertex in &mut polyline.vertices {
                    rotate_xy(
                        &mut vertex.location.x,
                        &mut vertex.location.y,
                        center,
                        quarter_turns,
                    );
                }
            }
            _ => {}
        }
    }
}

fn rotate_xy(x: &mut f64, y: &mut f64, center: Vector2, quarter_turns: u8) {
    let dx = *x - center.x;
    let dy = *y - center.y;
    (*x, *y) = match quarter_turns % 4 {
        1 => (center.x + dy, center.y - dx),
        2 => (center.x - dx, center.y - dy),
        3 => (center.x - dy, center.y + dx),
        _ => (*x, *y),
    };
}

fn entity_bounds(entities: &[EntityType]) -> Option<(Vector2, Vector2)> {
    let mut min = Vector2::new(f64::INFINITY, f64::INFINITY);
    let mut max = Vector2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut found = false;
    let mut include = |x: f64, y: f64| {
        min.x = min.x.min(x);
        min.y = min.y.min(y);
        max.x = max.x.max(x);
        max.y = max.y.max(y);
        found = true;
    };
    for entity in entities {
        match entity {
            EntityType::Line(line) => {
                include(line.start.x, line.start.y);
                include(line.end.x, line.end.y);
            }
            EntityType::LwPolyline(polyline) => {
                for vertex in &polyline.vertices {
                    include(vertex.location.x, vertex.location.y);
                }
            }
            _ => {}
        }
    }
    found.then_some((min, max))
}

pub fn entity_vertex_count(entity: &EntityType) -> usize {
    match entity {
        EntityType::Line(_) => 2,
        EntityType::LwPolyline(polyline) => polyline.vertices.len(),
        _ => 0,
    }
}

fn process_operations(
    document: &Document,
    content: &Content,
    resources: &[&Dictionary],
    state: &mut PageState,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("el PDF contiene formularios vectoriales anidados en exceso".to_string());
    }
    for operation in &content.operations {
        let numbers: Vec<f64> = operation.operands.iter().filter_map(number).collect();
        match operation.operator.as_str() {
            "q" => state.stack.push(state.ctm),
            "Q" => state.ctm = state.stack.pop().unwrap_or(Matrix::IDENTITY),
            "cm" if numbers.len() >= 6 => state.ctm = state.ctm.then(Matrix::from_pdf(&numbers)),
            "m" if numbers.len() >= 2 => state.move_to(Point {
                x: numbers[0],
                y: numbers[1],
            }),
            "l" if numbers.len() >= 2 => state.line_to(Point {
                x: numbers[0],
                y: numbers[1],
            }),
            "c" if numbers.len() >= 6 => state.curve_to(
                Point {
                    x: numbers[0],
                    y: numbers[1],
                },
                Point {
                    x: numbers[2],
                    y: numbers[3],
                },
                Point {
                    x: numbers[4],
                    y: numbers[5],
                },
            ),
            "v" if numbers.len() >= 4 => {
                let start = state.current_point().unwrap_or(Point { x: 0.0, y: 0.0 });
                let inv = inverse_apply(state.ctm, start);
                state.curve_to(
                    inv,
                    Point {
                        x: numbers[0],
                        y: numbers[1],
                    },
                    Point {
                        x: numbers[2],
                        y: numbers[3],
                    },
                );
            }
            "y" if numbers.len() >= 4 => {
                let end = Point {
                    x: numbers[2],
                    y: numbers[3],
                };
                state.curve_to(
                    Point {
                        x: numbers[0],
                        y: numbers[1],
                    },
                    end,
                    end,
                );
            }
            "h" => state.close(),
            "re" if numbers.len() >= 4 => {
                state.rectangle(numbers[0], numbers[1], numbers[2], numbers[3])
            }
            "S" | "f" | "F" | "f*" | "B" | "B*" => state.paint(false),
            "s" | "b" | "b*" => state.paint(true),
            "n" => state.discard_path(),
            "Tj" | "TJ" | "'" | "\"" => state.text_operators += 1,
            "Do" => {
                let Some(Object::Name(name)) = operation.operands.first() else {
                    continue;
                };
                let Some(stream) = lookup_xobject(document, resources, name) else {
                    continue;
                };
                match stream.dict.get(b"Subtype") {
                    Ok(Object::Name(subtype)) if subtype == b"Form" => {
                        process_form(document, stream, resources, state, depth + 1)?;
                    }
                    _ => state.image_operators += 1,
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn process_form(
    document: &Document,
    stream: &Stream,
    parent_resources: &[&Dictionary],
    state: &mut PageState,
    depth: usize,
) -> Result<(), String> {
    let saved_ctm = state.ctm;
    let saved_stack_len = state.stack.len();
    if let Ok(matrix_object) = stream.dict.get(b"Matrix") {
        if let Ok(values) = matrix_object.as_array() {
            let numbers: Vec<f64> = values.iter().filter_map(number).collect();
            if numbers.len() >= 6 {
                state.ctm = state.ctm.then(Matrix::from_pdf(&numbers));
            }
        }
    }

    let mut resources = Vec::new();
    if let Ok(object) = stream.dict.get(b"Resources") {
        if let Some(dictionary) = resolve_dictionary(document, object) {
            resources.push(dictionary);
        }
    }
    resources.extend_from_slice(parent_resources);

    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let content = Content::decode(&bytes).map_err(|e| e.to_string())?;
    let result = process_operations(document, &content, &resources, state, depth);
    state.ctm = saved_ctm;
    state.stack.truncate(saved_stack_len);
    result
}

fn lookup_xobject<'a>(
    document: &'a Document,
    resources: &[&'a Dictionary],
    name: &[u8],
) -> Option<&'a Stream> {
    for resource in resources {
        let Some(xobjects) = resource
            .get(b"XObject")
            .ok()
            .and_then(|object| resolve_dictionary(document, object))
        else {
            continue;
        };
        if let Ok(object) = xobjects.get(name) {
            if let Some(stream) = resolve_stream(document, object) {
                return Some(stream);
            }
        }
    }
    None
}

fn resolve_dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        _ => None,
    }
}

fn resolve_stream<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Stream> {
    match object {
        Object::Stream(stream) => Some(stream),
        Object::Reference(id) => document.get_object(*id).ok()?.as_stream().ok(),
        _ => None,
    }
}

fn inverse_apply(matrix: Matrix, point: Point) -> Point {
    let det = matrix.a * matrix.d - matrix.b * matrix.c;
    if det.abs() < 1e-12 {
        return point;
    }
    let x = point.x - matrix.e;
    let y = point.y - matrix.f;
    Point {
        x: (matrix.d * x - matrix.c * y) / det,
        y: (-matrix.b * x + matrix.a * y) / det,
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

fn page_width_points(document: &Document, page_id: lopdf::ObjectId) -> Option<f64> {
    let values = inherited_page_array(document, page_id, b"CropBox")
        .or_else(|| inherited_page_array(document, page_id, b"MediaBox"))?;
    if values.len() < 4 {
        return None;
    }
    Some(number(&values[2])? - number(&values[0])?)
}

fn inherited_page_array<'a>(
    document: &'a Document,
    mut object_id: lopdf::ObjectId,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    for _ in 0..32 {
        let dictionary = document.get_dictionary(object_id).ok()?;
        if let Ok(array) = dictionary.get(key).and_then(Object::as_array) {
            return Some(array);
        }
        object_id = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok()?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{content::Operation, dictionary};

    #[test]
    fn pdf_points_are_converted_to_millimetres() {
        let state = PageState::new(0.0);
        let p = state.ctm.apply(Point { x: 72.0, y: 36.0 });
        assert!((p.x - 25.4).abs() < 1e-9);
        assert!((p.y - 12.7).abs() < 1e-9);
    }

    #[test]
    fn rectangle_becomes_closed_polyline() {
        let mut state = PageState::new(0.0);
        state.rectangle(0.0, 0.0, 10.0, 20.0);
        state.paint(false);
        let entities = paths_to_entities(state.painted_paths);
        assert_eq!(entities.len(), 1);
        match &entities[0] {
            EntityType::LwPolyline(polyline) => {
                assert!(polyline.is_closed);
                assert_eq!(polyline.vertices.len(), 4);
            }
            _ => panic!("expected lightweight polyline"),
        }
    }

    #[test]
    fn cubic_curve_ends_at_requested_point() {
        let mut state = PageState::new(0.0);
        state.move_to(Point { x: 0.0, y: 0.0 });
        state.curve_to(
            Point { x: 0.0, y: 10.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 10.0, y: 0.0 },
        );
        let last = state.current_point().unwrap();
        assert!((last.x - 10.0 * POINT_TO_MM).abs() < 1e-9);
        assert!(last.y.abs() < 1e-9);
    }

    #[test]
    fn reads_vector_line_from_a_real_pdf_file() {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("m", vec![0.into(), 0.into()]),
                Operation::new("l", vec![72.into(), 0.into()]),
                Operation::new("S", vec![]),
            ],
        };
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode content"),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "Resources" => dictionary! {},
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);

        let path =
            std::env::temp_dir().join(format!("opencad-pdf-import-{}.pdf", std::process::id()));
        document.save(&path).expect("save fixture PDF");
        let result = read_pdf(&path).expect("import fixture PDF");
        let _ = std::fs::remove_file(path);

        assert_eq!(result.pages, 1);
        assert_eq!(result.entities.len(), 1);
        match &result.entities[0] {
            EntityType::Line(line) => {
                assert!((line.end.x - 25.4).abs() < 1e-9);
            }
            _ => panic!("expected line"),
        }
    }

    #[test]
    fn imports_vector_form_xobject_with_its_transform() {
        let mut document = Document::with_version("1.5");
        let form_content = Content {
            operations: vec![
                Operation::new("m", vec![0.into(), 0.into()]),
                Operation::new("l", vec![10.into(), 0.into()]),
                Operation::new("S", vec![]),
            ],
        };
        let form_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                "Matrix" => vec![2.into(), 0.into(), 0.into(), 2.into(), 5.into(), 0.into()],
            },
            form_content.encode().expect("encode form"),
        ));
        let resources = dictionary! {
            "XObject" => dictionary! { "Plan" => form_id },
        };
        let page_content = Content {
            operations: vec![Operation::new("Do", vec![Object::Name(b"Plan".to_vec())])],
        };
        let mut state = PageState::new(0.0);
        process_operations(&document, &page_content, &[&resources], &mut state, 0)
            .expect("process form");

        let entities = paths_to_entities(state.painted_paths);
        assert_eq!(entities.len(), 1);
        match &entities[0] {
            EntityType::Line(line) => {
                assert!((line.start.x - 5.0 * POINT_TO_MM).abs() < 1e-9);
                assert!((line.end.x - 25.0 * POINT_TO_MM).abs() < 1e-9);
            }
            _ => panic!("expected line"),
        }
    }

    #[test]
    fn preserves_coincident_and_connected_paths_as_distinct_geometry() {
        let paths = vec![
            Subpath {
                points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }],
                closed: false,
            },
            Subpath {
                points: vec![Point { x: 2.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }],
                closed: false,
            },
            Subpath {
                points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }],
                closed: false,
            },
        ];
        let entities = paths_to_entities(paths);
        assert_eq!(entities.len(), 3);
        assert!(entities
            .iter()
            .all(|entity| matches!(entity, EntityType::Line(_))));
    }

    #[test]
    fn quarter_turn_rotation_preserves_distances() {
        let entities = vec![EntityType::Line(Line::from_points(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(20.0, 10.0, 0.0),
        ))];
        let rotated = rotated_entities(entities, 1);
        let EntityType::Line(line) = &rotated[0] else {
            panic!("expected line");
        };
        assert!((line.start.distance(&line.end) - 500.0_f64.sqrt()).abs() < 1e-9);
        assert!((line.start.x - 5.0).abs() < 1e-9);
        assert!((line.start.y - 15.0).abs() < 1e-9);
    }

    #[test]
    fn dense_preview_is_bounded() {
        let entities: Vec<_> = (0..1_000)
            .map(|index| {
                EntityType::Line(Line::from_points(
                    Vector3::new(index as f64, 0.0, 0.0),
                    Vector3::new(index as f64, index as f64 + 1.0, 0.0),
                ))
            })
            .collect();
        let preview = preview_entities(&entities, 1);
        assert!(preview.len() <= PREVIEW_ENTITY_LIMIT);
        assert!(preview.len() >= 200);
    }

    #[test]
    #[ignore = "manual diagnostic for a user-supplied PDF"]
    fn diagnostic_real_pdf_complexity() {
        let path = std::env::var_os("PDF_IMPORT_DIAGNOSTIC")
            .map(std::path::PathBuf::from)
            .expect("set PDF_IMPORT_DIAGNOSTIC");
        let mut files = if path.is_dir() {
            std::fs::read_dir(&path)
                .expect("read diagnostic directory")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
                })
                .collect::<Vec<_>>()
        } else {
            vec![path]
        };
        files.sort();
        for file in files {
            let started = std::time::Instant::now();
            let result = read_pdf(&file).expect("read diagnostic PDF");
            let lines = result
                .entities
                .iter()
                .filter(|entity| matches!(entity, EntityType::Line(_)))
                .count();
            let polylines = result.entities.len() - lines;
            eprintln!(
                "{}: source_paths={} entities={} lines={} polylines={} vertices={} pages={} elapsed_ms={}",
                file.file_name().unwrap().to_string_lossy(),
                result.source_paths,
                result.entities.len(),
                lines,
                polylines,
                result.vertices,
                result.pages,
                started.elapsed().as_millis(),
            );
        }
    }
}
