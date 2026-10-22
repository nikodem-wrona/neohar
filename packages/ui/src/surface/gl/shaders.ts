// The two shaders of the pane program.
//
// One instanced draw paints a pane: instance i is the cell at column i % cols and row i / cols,
// and the vertex shader reads the cell word, the style id and the style from data textures. It
// emits one quad for the cell, or one quad of two cells for a wide glyph, or nothing for the
// spacer of a wide glyph. The fragment shader reads the glyph from the atlas and draws underline,
// strikethrough and the cursor as primitives. There are no vertex attributes.
//
// The cell word (`proto-wasm` `decoder.rs`): the atlas slot in bits 0 to 23, wide at bit 24,
// spacer at bit 25, cursor at bit 26. The style (four `u32`): foreground, background and
// underline colour as 0xRRGGBBAA, then the attribute bits (bold 1, dim 2, italic 4, inverse 8,
// strikethrough 16, hidden 32, blink 64) and the underline kind from bit 8 (none, single, double,
// curly, dotted, dashed). Inverse video is already applied to the colours.

/** The texture units, in the order the program expects them. */
export const UNIT = { cells: 0, ids: 1, styles: 2, kinds: 3, atlas: 4 } as const;

export const VERTEX_SOURCE = /* glsl */ `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;

uniform usampler2D uCells;    // R32UI, one texel per cell
uniform usampler2D uIds;      // R16UI, the style id of each cell
uniform usampler2D uStyles;   // RGBA32UI, 256 styles per row
uniform usampler2D uKinds;    // R8UI, 128 slots per row: 1 when the glyph has its own colours
uniform ivec2 uGrid;          // columns, rows
uniform vec2 uCell;           // cell size in pixels
uniform vec2 uOrigin;         // top left corner of the pane on the canvas, in pixels
uniform vec2 uCanvas;         // size of the canvas in pixels
uniform ivec4 uSel;           // selection: first column, first row, last column, last row
uniform int uSelMode;         // 0 none, 1 stream of cells, 2 rectangle
uniform vec4 uSelColor;       // colour and strength of the selection
uniform ivec2 uCursor;        // [0] 1 when the cursor shows now, [1] shape: block, underline, bar, hollow
uniform vec3 uCursorColor;
uniform int uBlink;           // 1 in the phase of a blink in which blinking text shows

flat out vec4 vFg;
flat out vec4 vBg;
flat out vec4 vUl;
flat out vec3 vCursor;
flat out ivec4 vA;            // slot, bit 0 glyph shows + bit 1 glyph has colours, underline kind, cursor primitive
flat out int vStrike;
flat out int vQuadW;
out vec2 vLocal;

vec4 unpackColor(uint c) {
  return vec4(float((c >> 24) & 255u), float((c >> 16) & 255u),
              float((c >> 8) & 255u), float(c & 255u)) / 255.0;
}

void main() {
  int col = gl_InstanceID % uGrid.x;
  int row = gl_InstanceID / uGrid.x;
  uint cell = texelFetch(uCells, ivec2(col, row), 0).r;
  vFg = vec4(0.0); vBg = vec4(0.0); vUl = vec4(0.0); vCursor = vec3(0.0);
  vA = ivec4(0); vStrike = 0; vQuadW = 0; vLocal = vec2(0.0);
  if ((cell & 0x2000000u) != 0u) {
    // The spacer of a wide glyph: the quad of the wide cell covers it.
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  bool wide = (cell & 0x1000000u) != 0u;
  uint id = texelFetch(uIds, ivec2(col, row), 0).r;
  uvec4 st = texelFetch(uStyles, ivec2(int(id & 255u), int(id >> 8)), 0);
  vec4 fg = unpackColor(st.x);
  vec4 bg = unpackColor(st.y);
  vec4 ul = unpackColor(st.z);
  uint attr = st.w & 255u;
  int underline = int((st.w >> 8) & 7u);
  bool hidden = (attr & 32u) != 0u;
  if ((attr & 2u) != 0u) fg.rgb = mix(bg.rgb, fg.rgb, 0.6);

  bool selected = false;
  if (uSelMode == 1) {
    selected = (row > uSel.y || (row == uSel.y && col >= uSel.x))
            && (row < uSel.w || (row == uSel.w && col <= uSel.z));
  } else if (uSelMode == 2) {
    selected = row >= uSel.y && row <= uSel.w && col >= uSel.x && col <= uSel.z;
  }
  if (selected) bg.rgb = mix(bg.rgb, uSelColor.rgb, uSelColor.a);

  int slot = int(cell & 0xFFFFFFu);
  int kind = 0;
  if (slot != 0) kind = int(texelFetch(uKinds, ivec2(slot & 127, slot >> 7), 0).r);
  bool blinkOff = (attr & 64u) != 0u && uBlink == 0;
  int shows = (slot != 0 && !hidden && !blinkOff) ? 1 : 0;

  int primitive = 0;
  if ((cell & 0x4000000u) != 0u && uCursor.x != 0) {
    if (uCursor.y == 0) {
      // A block cursor swaps the colours: the cell shows its background on the cursor colour.
      fg = vec4(bg.rgb, 1.0);
      bg = vec4(uCursorColor, 1.0);
    } else {
      primitive = uCursor.y;
    }
  }

  vFg = fg; vBg = bg; vUl = ul; vCursor = uCursorColor;
  vA = ivec4(slot, shows | (kind << 1), hidden ? 0 : underline, primitive);
  vStrike = ((attr & 16u) != 0u && !hidden) ? 1 : 0;
  vQuadW = wide ? int(uCell.x) * 2 : int(uCell.x);

  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 size = vec2(float(vQuadW), uCell.y);
  vec2 pos = uOrigin + vec2(float(col), float(row)) * uCell + corner * size;
  vLocal = corner * size;
  gl_Position = vec4(pos.x / uCanvas.x * 2.0 - 1.0, 1.0 - pos.y / uCanvas.y * 2.0, 0.0, 1.0);
}
`;

export const FRAGMENT_SOURCE = /* glsl */ `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2DArray;

uniform sampler2DArray uAtlasTex;  // RGBA8 pages of whole cells
uniform ivec2 uAtlas;              // slots in a row of a page, slots in a page
uniform vec2 uCell;
uniform ivec4 uLines;              // underline y, line thickness, strikethrough y

flat in vec4 vFg;
flat in vec4 vBg;
flat in vec4 vUl;
flat in vec3 vCursor;
flat in ivec4 vA;
flat in int vStrike;
flat in int vQuadW;
in vec2 vLocal;
out vec4 outColor;

void main() {
  ivec2 p = ivec2(vLocal);
  int cw = int(uCell.x);
  int ch = int(uCell.y);
  int lh = max(uLines.y, 1);
  vec3 c = vBg.rgb;

  if ((vA.y & 1) != 0) {
    // The right half of a wide glyph is in the slot after the left half.
    int slot = vA.x;
    int lx = p.x;
    if (p.x >= cw) { slot += 1; lx = p.x - cw; }
    int page = slot / uAtlas.y;
    int index = slot - page * uAtlas.y;
    ivec3 at = ivec3((index % uAtlas.x) * cw + lx, (index / uAtlas.x) * ch + p.y, page);
    vec4 t = texelFetch(uAtlasTex, at, 0);
    c = ((vA.y & 2) != 0) ? mix(c, t.rgb, t.a) : mix(c, vFg.rgb, t.a);
  }

  int u = vA.z;
  if (u != 0) {
    int uy = uLines.x;
    bool on = false;
    if (u == 1) {
      on = p.y >= uy && p.y < uy + lh;
    } else if (u == 2) {
      int y1 = min(uy, ch - 3 * lh);
      on = (p.y >= y1 && p.y < y1 + lh) || (p.y >= y1 + 2 * lh && p.y < y1 + 3 * lh);
    } else if (u == 3) {
      // A wave with integer arithmetic only, so that Canvas2D draws the same pixels: a bump up
      // and a bump down in each cell, in sixteenths of a pixel.
      int amp = max(1, lh);
      int base16 = 16 * min(uy, ch - lh - amp - 1);
      int v = (2 * p.x) % cw;
      int bump = v * (cw - v);
      int signedBump = ((2 * p.x) % (2 * cw) < cw) ? bump : -bump;
      int wave16 = ((signedBump + cw * cw / 4) * amp * 32) / (cw * cw);
      int center16 = base16 + wave16 - 8 * amp;
      on = abs(16 * p.y + 8 - center16 - 8 * lh) < 8 * lh + 8;
    } else if (u == 4) {
      on = p.y >= uy && p.y < uy + lh && ((p.x / lh) % 2) == 0;
    } else {
      on = p.y >= uy && p.y < uy + lh && (p.x % cw) < cw - max(cw / 4, 1);
    }
    if (on) c = vUl.rgb;
  }

  if (vStrike != 0 && p.y >= uLines.z && p.y < uLines.z + lh) c = vFg.rgb;

  int shape = vA.w;
  if (shape != 0) {
    bool on = false;
    if (shape == 1) on = p.y >= ch - max(lh, ch / 8);
    else if (shape == 2) on = p.x < max(lh, cw / 8);
    else on = p.x < lh || p.x >= vQuadW - lh || p.y < lh || p.y >= ch - lh;
    if (on) c = vCursor;
  }

  outColor = vec4(c, 1.0);
}
`;

// The program of the shapes: the main panel and the background of each pane, as rounded
// rectangles. One quad covers the rectangle, and the fragment shader keeps the pixels inside the
// round corners. The edge of one pixel blends into the colour under it. The distance is the one of
// `roundedDistance` in `geometry.ts`.

export const SHAPE_VERTEX_SOURCE = /* glsl */ `#version 300 es
precision highp float;

uniform vec4 uRect;     // x, y, width, height in pixels, from the top left of the canvas
uniform vec2 uCanvas;   // size of the canvas in pixels

void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vec2 px = uRect.xy + corner * uRect.zw;
  vec2 clip = px / uCanvas * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
}
`;

export const SHAPE_FRAGMENT_SOURCE = /* glsl */ `#version 300 es
precision highp float;

uniform vec4 uRect;     // x, y, width, height in pixels, from the top left of the canvas
uniform vec4 uRadii;    // top left, top right, bottom right, bottom left
uniform vec4 uColor;
uniform vec2 uCanvas;

out vec4 outColor;

void main() {
  vec2 px = vec2(gl_FragCoord.x, uCanvas.y - gl_FragCoord.y);
  vec2 half_ = uRect.zw * 0.5;
  vec2 p = px - (uRect.xy + half_);
  float r = p.x < 0.0 ? (p.y < 0.0 ? uRadii.x : uRadii.w) : (p.y < 0.0 ? uRadii.y : uRadii.z);
  vec2 q = abs(p) - half_ + r;
  float d = min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
  float a = clamp(0.5 - d, 0.0, 1.0);
  if (a <= 0.0) discard;
  outColor = vec4(uColor.rgb, uColor.a * a);
}
`;
