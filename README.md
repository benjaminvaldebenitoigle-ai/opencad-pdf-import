# PDF a CAD para Open CAD Studio 2026.36

Complemento nativo que importa trazados vectoriales PDF en el dibujo activo de
Open CAD Studio como entidades `LINE` y `LWPOLYLINE` editables.

## Funciones

- vista previa ligera integrada en el dibujo antes de insertar;
- rotación de la vista previa en pasos de 90° a izquierda o derecha;
- confirmación o cancelación sin dejar geometría temporal;
- escala física correcta: 72 puntos PDF = 25,4 mm;
- líneas, polilíneas, rectángulos y contornos cerrados;
- curvas Bézier aproximadas con 12 segmentos;
- formularios PDF vectoriales (`Form XObject`) y documentos de varias páginas;
- una única operación de deshacer para toda la importación;
- importación fiel: conserva separados incluso los trazados coincidentes y no
  elimina ni combina vértices; usa `LINE` para cada trazado simple de dos puntos;
- diagnóstico para PDF cifrado o sin geometría vectorial.

Las páginas se colocan de izquierda a derecha con 10 mm de separación. El texto
y las imágenes raster se omiten y se informa su cantidad al terminar. Un PDF
escaneado debe vectorizarse antes de importarlo.

## Compatibilidad

Este código está fijado al commit usado para compilar la versión oficial de
Open CAD Studio `2026.36`,
`afbf826b0cf5bca44acf61a05be278cd673d7d39`, API de complementos 5 y cadcodec
`5b56571a190e7a17c8f12d36390d2938b0fb72f7`.

Los complementos nativos deben compilarse con la misma versión exacta de Rust
que el programa anfitrión. La versión oficial 2026.36 usa:

`rustc 1.98.1 (48a229cea 2026-09-01)`

## Compilar

Instale Rust 1.98.1 y las herramientas C++ de Visual Studio para Windows:

```powershell
rustup toolchain install 1.98.1-x86_64-pc-windows-msvc
cargo +1.98.1-x86_64-pc-windows-msvc build --release --locked
```

El binario se genera en `target/release/opencad_pdf_import.dll`.

En Windows también puede ejecutar `scripts/build-windows.ps1`; el script
verifica el compilador exacto y crea el ZIP instalable dentro de `dist`.

## Instalar manualmente en Windows

1. Cree `%APPDATA%\OpenCADStudio\plugins\opencad.pdf_import\`.
2. Copie allí `plugin.toml` y `opencad.pdf_import-windows-x86_64.dll`.
3. Reinicie Open CAD Studio.
4. Abra un dibujo nuevo o existente y use **PDF a CAD > Vista previa**.
5. Pulse **Ajustar vista** si el plano no cabe en pantalla y use **Girar
   izquierda** o **Girar derecha** hasta obtener la orientación deseada.
6. Pulse **Insertar plano** para confirmar, o **Cancelar** para retirar la vista
   previa.
7. Guarde el dibujo como DXF o DWG desde Open CAD Studio.

La vista previa muestra una muestra representativa limitada a 240 entidades,
para que incluso los planos muy densos puedan rotarse con fluidez. Al confirmar
se inserta la geometría vectorial completa sin eliminar ni fusionar trazados.

Open CAD Studio 2026.36 puede representar símbolos repetidos eficientemente
como bloques, pero su API 5 para complementos todavía no permite crear las
definiciones de esos bloques. Por seguridad, este complemento no reemplaza un
patrón repetido hasta que el anfitrión pueda recibir su definición y sus
inserciones de forma atómica.

## Pruebas

```powershell
cargo +1.98.1-x86_64-pc-windows-msvc test --locked
```

## Licencia

GPL-3.0-only, compatible con Open CAD Studio.
