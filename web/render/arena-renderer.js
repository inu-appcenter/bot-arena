const COLORS = {
  ink: "#26382f", muted: "#929b8e", line: "#e2e6da", ground: "#fcfdf7", alternating: "#f6f8ef",
  a: "#1c7467", aLight: "#e4eee3", b: "#ca7258", bLight: "#f7eae0", resource: "#d5a63a", resourceLight: "#fff0c4",
};

function rounded(context, x, y, width, height, radius, fill, stroke) {
  context.beginPath();
  context.roundRect(x, y, width, height, radius);
  if (fill) { context.fillStyle = fill; context.fill(); }
  if (stroke) { context.strokeStyle = stroke; context.stroke(); }
}

function resource(context, x, y, cell, amount, occupied) {
  const size = cell * (occupied ? .25 : .57);
  const left = occupied ? x + cell * .72 : x + (cell - size) / 2;
  const top = occupied ? y + cell * .035 : y + (cell - size) / 2;
  rounded(context, left, top, size, size, size * .23, COLORS.resourceLight, "#e7cd83");
  const unit = size * .2;
  for (let index = 0; index < amount; index += 1) {
    const dx = left + size * .23 + (index % 2) * size * .34;
    const dy = top + size * .23 + Math.floor(index / 2) * size * .34;
    rounded(context, dx, dy, unit, unit, unit * .22, COLORS.resource);
  }
}

function robot(context, x, y, cell, value, capacity) {
  const color = value.team === "A" ? COLORS.a : COLORS.b;
  const size = cell * .76;
  const left = x + (cell - size) / 2;
  const top = y + (cell - size) / 2;
  context.save();
  context.shadowColor = "rgba(38, 56, 47, .14)";
  context.shadowBlur = cell * .15;
  context.shadowOffsetY = cell * .065;
  rounded(context, left, top, size, size, cell * .20, color);
  context.restore();
  // Letters identify teams independently of color; the four pips show cargo.
  context.fillStyle = "#fffefa";
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.font = `700 ${Math.max(8, cell * .28)}px -apple-system, sans-serif`;
  context.fillText(value.id, x + cell / 2, y + cell * .43);
  const pip = cell * .095;
  const gap = cell * .035;
  const total = capacity * pip + (capacity - 1) * gap;
  for (let index = 0; index < capacity; index += 1) {
    rounded(context, x + (cell - total) / 2 + index * (pip + gap), y + cell * .65, pip, pip, pip * .25, index < value.cargo ? "#f2cd6d" : "rgba(255, 254, 250, .25)");
  }
}

/** Draws authoritative engine state only. It never predicts actions or scores. */
export function renderArena(canvas, state) {
  if (!state) return;
  const bounds = canvas.getBoundingClientRect();
  const side = bounds.width;
  if (!side) return;
  const pixelRatio = window.devicePixelRatio || 1;
  const pixels = Math.round(side * pixelRatio);
  if (canvas.width !== pixels || canvas.height !== pixels) { canvas.width = pixels; canvas.height = pixels; }
  const context = canvas.getContext("2d");
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  context.clearRect(0, 0, side, side);
  const margin = side * .044;
  const size = side - margin * 2;
  const cell = size / state.width;
  context.lineWidth = 1;
  for (let y = 0; y < state.height; y += 1) {
    for (let x = 0; x < state.width; x += 1) {
      context.fillStyle = x === 0 ? COLORS.aLight : x === state.width - 1 ? COLORS.bLight : (x + y) % 2 ? COLORS.ground : COLORS.alternating;
      context.fillRect(margin + x * cell, margin + y * cell, cell, cell);
    }
  }
  context.strokeStyle = COLORS.line;
  context.lineWidth = .7;
  context.beginPath();
  for (let i = 0; i <= state.width; i += 1) {
    const position = margin + i * cell;
    context.moveTo(position, margin); context.lineTo(position, margin + size);
    context.moveTo(margin, position); context.lineTo(margin + size, position);
  }
  context.stroke();
  rounded(context, margin, margin, size, size, 2, null, "#d6ddcf");

  context.fillStyle = COLORS.muted;
  context.font = `${Math.max(8, cell * .23)}px -apple-system, sans-serif`;
  context.textAlign = "center";
  context.textBaseline = "middle";
  for (let i = 0; i < state.width; i += 1) {
    context.fillText(String(i), margin + (i + .5) * cell, margin / 2);
    context.fillText(String(i), margin / 2, margin + (i + .5) * cell);
  }
  context.font = `700 ${cell * .32}px -apple-system, sans-serif`;
  for (const y of [1, 5, 9, 13]) {
    context.fillStyle = "rgba(28, 116, 103, .24)";
    context.fillText("A", margin + cell / 2, margin + (y + .5) * cell);
    context.fillStyle = "rgba(202, 114, 88, .24)";
    context.fillText("B", margin + (state.width - .5) * cell, margin + (y + .5) * cell);
  }
  for (const obstacle of state.obstacles) {
    rounded(context, margin + (obstacle.x + .15) * cell, margin + (obstacle.y + .15) * cell, cell * .7, cell * .7, cell * .12, "#9aab93");
  }
  const occupied = new Set(state.robots.map(value => `${value.x},${value.y}`));
  for (const value of state.resources) {
    if (value.amount > 0) resource(context, margin + value.x * cell, margin + value.y * cell, cell, value.amount, occupied.has(`${value.x},${value.y}`));
  }
  for (const value of state.robots) {
    robot(context, margin + value.x * cell, margin + value.y * cell, cell, value, state.capacity);
  }
}
