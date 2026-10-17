// State
let queryHistory = [];
let graphNodes = [];
let graphEdges = [];
let selectedEntity = null;
let selectedRowNodeIds = new Set();
let selectedRowEdges = new Set();
let physicsRunning = true;

// Canvas & Camera State
const canvas = document.getElementById('graph-canvas');
const ctx = canvas ? canvas.getContext('2d') : null;
let width = canvas ? canvas.clientWidth : 800;
let height = canvas ? canvas.clientHeight : 600;
let cameraX = 0;
let cameraY = 0;
let cameraZoom = 1.0;
let isDragging = false;
let dragStart = { x: 0, y: 0 };
let draggedNode = null;
let hoveredNode = null;

// Resize Canvas with high-DPI transform reset & ResizeObserver
function resizeCanvas() {
  if (!canvas || !canvas.parentElement) return;
  const rect = canvas.parentElement.getBoundingClientRect();
  width = rect.width;
  height = rect.height;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(width * dpr);
  canvas.height = Math.floor(height * dpr);
  canvas.style.width = width + 'px';
  canvas.style.height = height + 'px';
  if (ctx) {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.scale(dpr, dpr);
  }
}
window.addEventListener('resize', resizeCanvas);
if (window.ResizeObserver && canvas && canvas.parentElement) {
  const ro = new ResizeObserver(() => resizeCanvas());
  ro.observe(canvas.parentElement);
}
setTimeout(resizeCanvas, 50);

// Color Palette
const COLORS = ['#58a6ff', '#bc8cff', '#3fb950', '#f0883e', '#39c5bb', '#f778ba', '#d29922', '#79c0ff'];
function getNodeColor(label, id) {
  if (typeof id === 'number') return COLORS[Math.abs(id) % COLORS.length];
  let hash = 0;
  const str = String(label || id);
  for (let i = 0; i < str.length; i++) hash = (hash << 5) - hash + str.charCodeAt(i);
  return COLORS[Math.abs(hash) % COLORS.length];
}

let currentView = 'graph';
let lastLoadedGraphData = null;
let simulationAlpha = 1.0;

function wakeSimulation(alpha = 0.6) {
  simulationAlpha = Math.max(simulationAlpha, alpha);
}

// Force Simulation
function stepSimulation() {
  if (!physicsRunning || graphNodes.length === 0 || currentView !== 'graph') return;
  if (simulationAlpha < 0.002) return;

  const alpha = simulationAlpha;
  const repulsion = 3500;
  const springLength = 90;
  const springStrength = 0.04;
  const maxRepulsionForce = 12.0;
  const maxSpringForce = 10.0;
  const maxSpeed = 7.0;

  // 1. Repulsion between nodes (with distance softening to prevent infinite explosion)
  for (let i = 0; i < graphNodes.length; i++) {
    const a = graphNodes[i];
    for (let j = i + 1; j < graphNodes.length; j++) {
      const b = graphNodes[j];
      let dx = b.x - a.x;
      let dy = b.y - a.y;
      let dist = Math.sqrt(dx * dx + dy * dy);

      if (dist < 0.001) {
        dx = (Math.random() - 0.5) * 2;
        dy = (Math.random() - 0.5) * 2;
        dist = Math.sqrt(dx * dx + dy * dy) || 1;
      }
      if (dist > 350) continue;

      const effDist = Math.max(dist, 25);
      const rawForce = repulsion / (effDist * effDist);
      const force = Math.min(rawForce, maxRepulsionForce) * alpha;

      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      a.vx -= fx;
      a.vy -= fy;
      b.vx += fx;
      b.vy += fy;
    }
  }

  // 2. Attraction along edges (with spring force clamping)
  for (const edge of graphEdges) {
    const a = edge.source;
    const b = edge.target;
    if (!a || !b) continue;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let dist = Math.sqrt(dx * dx + dy * dy);
    if (dist < 0.001) continue;

    const displacement = dist - springLength;
    const rawForce = displacement * springStrength;
    const force = Math.max(-maxSpringForce, Math.min(maxSpringForce, rawForce)) * alpha;

    const fx = (dx / dist) * force;
    const fy = (dy / dist) * force;
    a.vx += fx;
    a.vy += fy;
    b.vx -= fx;
    b.vy -= fy;
  }

  // 3. Center gravity & update positions
  for (const n of graphNodes) {
    if (n === draggedNode) continue;

    const distCenter = Math.hypot(n.x, n.y);
    if (distCenter > 40) {
      const gForce = Math.min((distCenter - 40) * 0.003, 1.2) * alpha;
      n.vx -= (n.x / distCenter) * gForce;
      n.vy -= (n.y / distCenter) * gForce;
    }

    const speed = Math.hypot(n.vx, n.vy);
    if (speed > maxSpeed) {
      n.vx = (n.vx / speed) * maxSpeed;
      n.vy = (n.vy / speed) * maxSpeed;
    }

    n.vx *= 0.82; // Damping
    n.vy *= 0.82;
    n.x += n.vx;
    n.y += n.vy;
  }

  // Temperature cooling
  simulationAlpha *= 0.985;
}

// Render Loop
function renderGraph() {
  if (!ctx || !canvas) return;
  ctx.clearRect(0, 0, width, height);

  ctx.save();
  // Apply Camera Transform
  ctx.translate(width / 2 + cameraX, height / 2 + cameraY);
  ctx.scale(cameraZoom, cameraZoom);

  // Draw Edges
  for (const edge of graphEdges) {
    const a = edge.source;
    const b = edge.target;
    if (!a || !b) continue;

    const isRowEdge = selectedRowEdges && selectedRowEdges.has(edge);
    ctx.strokeStyle = (selectedEntity === edge) ? '#58a6ff' : (isRowEdge ? '#39c5bb' : 'rgba(88, 166, 255, 0.35)');
    ctx.lineWidth = (selectedEntity === edge) ? 2.5 : (isRowEdge ? 2.5 : 1.5);

    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();

    // Draw Arrow
    const angle = Math.atan2(b.y - a.y, b.x - a.x);
    const nodeRadius = 18;
    const targetX = b.x - Math.cos(angle) * (nodeRadius + 2);
    const targetY = b.y - Math.sin(angle) * (nodeRadius + 2);

    ctx.fillStyle = ctx.strokeStyle;
    ctx.beginPath();
    ctx.moveTo(targetX, targetY);
    ctx.lineTo(targetX - 8 * Math.cos(angle - Math.PI / 6), targetY - 8 * Math.sin(angle - Math.PI / 6));
    ctx.lineTo(targetX - 8 * Math.cos(angle + Math.PI / 6), targetY - 8 * Math.sin(angle + Math.PI / 6));
    ctx.closePath();
    ctx.fill();

    // Edge label if present
    if (edge.label) {
      const midX = (a.x + b.x) / 2;
      const midY = (a.y + b.y) / 2;
      ctx.fillStyle = isRowEdge ? '#7ee787' : '#8b949e';
      ctx.font = isRowEdge ? 'bold 10px monospace' : '10px monospace';
      ctx.fillText(edge.label, midX + 4, midY - 4);
    }
  }

  // Draw Nodes
  for (const n of graphNodes) {
    const isHovered = (n === hoveredNode);
    const isSelected = (n === selectedEntity);
    const isRowHighlighted = selectedRowNodeIds && selectedRowNodeIds.has(String(n.id));
    const color = getNodeColor(n.label, n.id);

    // Glow halo on select/hover/row highlight
    if (isSelected || isHovered || isRowHighlighted) {
      ctx.beginPath();
      ctx.arc(n.x, n.y, isRowHighlighted ? 26 : 24, 0, Math.PI * 2);
      ctx.fillStyle = isSelected ? 'rgba(88, 166, 255, 0.35)' : (isRowHighlighted ? 'rgba(57, 197, 187, 0.35)' : 'rgba(255, 255, 255, 0.15)');
      ctx.fill();
    }

    // Node Circle
    ctx.beginPath();
    ctx.arc(n.x, n.y, 18, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.lineWidth = (isSelected || isRowHighlighted) ? 3 : 1.5;
    ctx.strokeStyle = isSelected ? '#ffffff' : (isRowHighlighted ? '#39c5bb' : '#0d1117');
    ctx.stroke();

    // Node Label
    ctx.fillStyle = '#f0f6fc';
    ctx.font = 'bold 11px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const displayLabel = String(n.label || n.id).substring(0, 5);
    ctx.fillText(displayLabel, n.x, n.y);

    // Subtext / ID below node
    ctx.fillStyle = '#8b949e';
    ctx.font = '10px monospace';
    ctx.fillText(String(n.id), n.x, n.y + 28);
  }

  ctx.restore();

  stepSimulation();
  requestAnimationFrame(renderGraph);
}
requestAnimationFrame(renderGraph);

// Mouse & Touch Controls
function screenToWorld(sx, sy) {
  return {
    x: (sx - width / 2 - cameraX) / cameraZoom,
    y: (sy - height / 2 - cameraY) / cameraZoom
  };
}

function findNodeAt(wx, wy) {
  for (let i = graphNodes.length - 1; i >= 0; i--) {
    const n = graphNodes[i];
    const dx = n.x - wx;
    const dy = n.y - wy;
    if (dx * dx + dy * dy <= 22 * 22) return n;
  }
  return null;
}

if (canvas) {
  canvas.addEventListener('mousedown', (e) => {
    const rect = canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const world = screenToWorld(sx, sy);
    const clicked = findNodeAt(world.x, world.y);

    if (clicked) {
      draggedNode = clicked;
      selectedEntity = clicked;
      openInspector(clicked);
    } else {
      isDragging = true;
      dragStart = { x: sx - cameraX, y: sy - cameraY };
    }
  });

  window.addEventListener('mousemove', (e) => {
    const rect = canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const world = screenToWorld(sx, sy);

    if (draggedNode) {
      draggedNode.x = world.x;
      draggedNode.y = world.y;
      draggedNode.vx = 0;
      draggedNode.vy = 0;
      wakeSimulation(0.4);
    } else if (isDragging) {
      cameraX = sx - dragStart.x;
      cameraY = sy - dragStart.y;
    } else {
      hoveredNode = findNodeAt(world.x, world.y);
    }
  });

  window.addEventListener('mouseup', () => {
    draggedNode = null;
    isDragging = false;
  });

  canvas.addEventListener('wheel', (e) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 1.1 : 0.9;
    cameraZoom = Math.max(0.2, Math.min(5.0, cameraZoom * zoomFactor));
  });
}

function zoomGraph(factor) {
  cameraZoom = Math.max(0.2, Math.min(5.0, cameraZoom * factor));
}
function resetGraphView() {
  cameraX = 0; cameraY = 0; cameraZoom = 1.0;
}
function togglePhysics() {
  physicsRunning = !physicsRunning;
  if (physicsRunning) wakeSimulation(1.0);
  const btn = document.getElementById('physics-btn');
  if (btn) btn.textContent = physicsRunning ? '⏸' : '▶';
}

// Graph Exports
function exportGraphPng() {
  if (graphNodes.length === 0) {
    alert("No graph elements to export.");
    return;
  }
  const link = document.createElement('a');
  link.download = `gdb-graph-${Date.now()}.png`;
  link.href = canvas.toDataURL('image/png');
  link.click();
}

function exportGraphSvg() {
  if (graphNodes.length === 0) {
    alert("No graph elements to export.");
    return;
  }
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of graphNodes) {
    if (n.x < minX) minX = n.x;
    if (n.x > maxX) maxX = n.x;
    if (n.y < minY) minY = n.y;
    if (n.y > maxY) maxY = n.y;
  }
  const pad = 60;
  const w = Math.max(maxX - minX + pad * 2, 400);
  const h = Math.max(maxY - minY + pad * 2, 400);
  const offX = -minX + pad;
  const offY = -minY + pad;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" style="background:#0d1117;">\n`;
  svg += `<defs>\n  <marker id="arrow" viewBox="0 0 10 10" refX="22" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">\n    <path d="M 0 0 L 10 5 L 0 10 z" fill="#58a6ff" />\n  </marker>\n</defs>\n`;

  for (const edge of graphEdges) {
    if (!edge.source || !edge.target) continue;
    const x1 = edge.source.x + offX;
    const y1 = edge.source.y + offY;
    const x2 = edge.target.x + offX;
    const y2 = edge.target.y + offY;
    svg += `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="rgba(88, 166, 255, 0.4)" stroke-width="1.5" marker-end="url(#arrow)" />\n`;
    if (edge.label) {
      const mx = (x1 + x2) / 2;
      const my = (y1 + y2) / 2;
      svg += `<text x="${mx + 3}" y="${my - 3}" fill="#8b949e" font-size="10" font-family="monospace">${edge.label}</text>\n`;
    }
  }

  for (const n of graphNodes) {
    const cx = n.x + offX;
    const cy = n.y + offY;
    const color = getNodeColor(n.label, n.id);
    svg += `<circle cx="${cx}" cy="${cy}" r="18" fill="${color}" stroke="#ffffff" stroke-width="1.5" />\n`;
    const lbl = String(n.label || n.id).substring(0, 5);
    svg += `<text x="${cx}" y="${cy + 4}" fill="#ffffff" font-size="11" font-family="sans-serif" font-weight="bold" text-anchor="middle">${lbl}</text>\n`;
    svg += `<text x="${cx}" y="${cy + 28}" fill="#8b949e" font-size="10" font-family="monospace" text-anchor="middle">${n.id}</text>\n`;
  }
  svg += `</svg>`;

  const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = `gdb-graph-${Date.now()}.svg`;
  link.click();
}

function exportGraphCsv() {
  if (!lastLoadedGraphData || !lastLoadedGraphData.columns) {
    alert("No query result data to export.");
    return;
  }
  const headers = lastLoadedGraphData.columns.join(',');
  const rows = (lastLoadedGraphData.rows || []).map(r => r.map(cell => {
    const str = typeof cell === 'object' ? JSON.stringify(cell) : String(cell);
    return `"${str.replace(/"/g, '""')}"`;
  }).join(','));
  const csv = [headers, ...rows].join('\n');
  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = `gdb-query-${Date.now()}.csv`;
  link.click();
}

function exportGraphJson() {
  if (!lastLoadedGraphData) {
    alert("No query result data to export.");
    return;
  }
  const blob = new Blob([JSON.stringify(lastLoadedGraphData, null, 2)], { type: 'application/json' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = `gdb-export-${Date.now()}.json`;
  link.click();
}

// Inspector View
function openInspector(entity) {
  const panel = document.getElementById('inspector-panel');
  const title = document.getElementById('inspector-title');
  const body = document.getElementById('inspector-body');
  if (!panel || !title || !body) return;
  panel.classList.remove('closed');

  title.textContent = `Node #${entity.id}`;
  let html = '';
  html += `<div class="prop-row"><div class="prop-key">ID</div><div class="prop-val">${entity.id}</div></div>`;
  if (entity.label) html += `<div class="prop-row"><div class="prop-key">Label / Tag</div><div class="prop-val">${entity.label}</div></div>`;

  if (entity.properties && Object.keys(entity.properties).length > 0) {
    html += `<div style="font-size:11px; font-weight:700; color:var(--text-muted); margin:10px 0 6px 0; text-transform:uppercase;">Properties</div>`;
    for (const [k, v] of Object.entries(entity.properties)) {
      const valStr = typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v);
      html += `<div class="prop-row"><div class="prop-key">${k}</div><div class="prop-val">${valStr}</div></div>`;
    }
  }

  // Show Connections
  const edges = graphEdges.filter(e => e.source === entity || e.target === entity);
  html += `<div class="prop-row" style="margin-top:10px;"><div class="prop-key">Connected Edges (${edges.length})</div>`;
  for (const e of edges) {
    const neighbor = (e.source === entity) ? e.target : e.source;
    const dir = (e.source === entity) ? '→' : '←';
    html += `<div style="font-size:11px; margin-top:4px;"><span style="color:var(--accent);">${dir} [${e.label || 'EDGE'}]</span> Node #${neighbor.id}</div>`;
  }
  html += `</div>`;

  body.innerHTML = html;
}

function closeInspector() {
  const panel = document.getElementById('inspector-panel');
  if (panel) panel.classList.add('closed');
  selectedEntity = null;
}

function focusNode(nodeId) {
  const targetId = String(nodeId);
  const node = graphNodes.find(n => String(n.id) === targetId);
  if (node) {
    cameraX = -node.x;
    cameraY = -node.y;
    selectedEntity = node;
    openInspector(node);
    wakeSimulation(0.5);
    if (typeof switchView === 'function') {
      switchView('graph', document.getElementById('tab-btn-graph'));
    }
  }
}

// Helper to parse entity from JSON string, object, or property scalar
function parseNodeEntity(rawVal, colName) {
  if (rawVal === null || rawVal === undefined) return null;
  const rawStr = String(rawVal).trim();
  if (rawStr === '' || rawStr === 'null' || rawStr === 'undefined') return null;

  let parsed = null;
  if (typeof rawVal === 'object' && rawVal !== null) {
    parsed = rawVal;
  } else if (typeof rawVal === 'string') {
    if (rawStr.startsWith('{') && rawStr.endsWith('}')) {
      try {
        parsed = JSON.parse(rawStr);
      } catch (e) {
        parsed = null;
      }
    }
  }

  if (parsed && typeof parsed === 'object') {
    let id = parsed.id !== undefined ? String(parsed.id) : (parsed.uid !== undefined ? String(parsed.uid) : null);
    let label = parsed.label || parsed.tag || (colName ? colName.split('.')[0] : 'Node');
    let properties = parsed.properties || {};
    if (!id && Object.keys(properties).length > 0) {
      id = properties.id !== undefined ? String(properties.id) : null;
    }
    if (!id) id = rawStr;
    return { id, label, properties };
  }

  let id = rawStr;
  let label = 'Node';
  let properties = {};
  if (colName) {
    if (colName.includes('.')) {
      const parts = colName.split('.');
      label = parts[0];
      properties[parts[1]] = rawVal;
    } else if (colName.toLowerCase().startsWith('id(')) {
      const varName = colName.replace(/^[iI][dD]\(/, '').replace(/\)$/, '').trim();
      label = varName || 'Node';
      properties['id'] = rawVal;
    } else if (colName.toLowerCase().startsWith('label(') || colName.toLowerCase().startsWith('labels(')) {
      label = rawStr;
    } else {
      label = colName;
    }
  }
  return { id, label, properties };
}

function getOrAddNode(nodeMap, candidate) {
  if (!candidate || !candidate.id) return null;
  const key = String(candidate.id);
  if (key === '' || key === 'null' || key === 'undefined') return null;

  if (nodeMap.has(key)) {
    const existing = nodeMap.get(key);
    if (candidate.label && (!existing.label || existing.label === 'Node' || existing.label === existing.id)) {
      existing.label = candidate.label;
    }
    if (candidate.properties) {
      existing.properties = Object.assign(existing.properties || {}, candidate.properties);
    }
    return existing;
  }
  const node = {
    id: key,
    label: candidate.label || key,
    properties: candidate.properties || {},
    x: (Math.random() - 0.5) * 200,
    y: (Math.random() - 0.5) * 200,
    vx: 0,
    vy: 0,
  };
  nodeMap.set(key, node);
  graphNodes.push(node);
  return node;
}

// Auto-extract graph from generic rows (e.g. [a.name, b.name] or [vertex_id, score] or [a, b])
function extractGraphFromRows(data) {
  if (!data || !data.columns || !data.rows || data.rows.length === 0) {
    graphNodes = [];
    graphEdges = [];
    selectedEntity = null;
    closeInspector();
    const statGraph = document.getElementById('stat-graph');
    if (statGraph) statGraph.textContent = '0 nodes, 0 edges';
    if (ctx && canvas) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
    }
    if (typeof sync3DGraph === 'function') sync3DGraph();
    if (currentView === 'unity' && typeof renderUnityGraph === 'function') renderUnityGraph();
    return;
  }

  const MAX_GRAPH_NODES = 250;
  let capped = false;
  const nodeMap = new Map();
  const edgeSet = new Set();
  graphNodes = [];
  graphEdges = [];

  // Case 1: Multi-column path traversal (e.g. 1-hop, 2-hop, 3-hop: a -> b -> c -> d)
  if (data.columns.length >= 2 && !data.columns[1].includes('score') && !data.columns[1].includes('distance')) {
    const isAlternating = (data.columns.length >= 3 && data.columns.length % 2 === 1 &&
      data.columns.slice(1).some((c, i) => i % 2 === 0 && (c.toLowerCase().includes('type') || c.toLowerCase().includes('rel') || c.toLowerCase().includes('edge'))));

    for (const row of data.rows) {
      if (graphNodes.length >= MAX_GRAPH_NODES) {
        capped = true;
        break;
      }

      if (isAlternating) {
        for (let step = 0; step < row.length - 2; step += 2) {
          const uNode = getOrAddNode(nodeMap, parseNodeEntity(row[step], data.columns[step]));
          const relType = String(row[step + 1] || '');
          const vNode = getOrAddNode(nodeMap, parseNodeEntity(row[step + 2], data.columns[step + 2]));

          if (uNode && vNode) {
            const edgeKey = `${uNode.id}->${vNode.id}:${relType}`;
            if (!edgeSet.has(edgeKey)) {
              edgeSet.add(edgeKey);
              graphEdges.push({
                source: uNode,
                target: vNode,
                label: (relType === 'null' || !relType) ? 'REL' : relType
              });
            }
          }
        }
      } else {
        // Consecutive hops along path: row[0] -> row[1] -> row[2] -> row[3] ...
        for (let step = 0; step < row.length - 1; step++) {
          const uNode = getOrAddNode(nodeMap, parseNodeEntity(row[step], data.columns[step]));
          const vNode = getOrAddNode(nodeMap, parseNodeEntity(row[step + 1], data.columns[step + 1]));

          if (uNode && vNode) {
            const edgeKey = `${uNode.id}->${vNode.id}:REL`;
            if (!edgeSet.has(edgeKey)) {
              edgeSet.add(edgeKey);
              graphEdges.push({
                source: uNode,
                target: vNode,
                label: 'REL'
              });
            }
          }
        }
      }
    }
  }
  // Case 2: Analytical single vertex score table (e.g. vertex_id, score/community)
  else if (data.columns.includes('vertex_id') || data.columns.includes('node')) {
    const idIdx = data.columns.indexOf('vertex_id') !== -1 ? data.columns.indexOf('vertex_id') : data.columns.indexOf('node');
    const metricIdx = idIdx === 0 ? 1 : 0;
    const metricName = data.columns[metricIdx] || 'score';

    for (const row of data.rows) {
      if (graphNodes.length >= MAX_GRAPH_NODES) {
        capped = true;
        break;
      }
      const vid = String(row[idIdx]);
      if (!vid || vid === 'null') continue;

      const metricVal = row[metricIdx];
      const node = {
        id: vid,
        label: `${vid} (${metricVal})`,
        properties: { [metricName]: metricVal },
        x: (Math.random() - 0.5) * 250,
        y: (Math.random() - 0.5) * 250,
        vx: 0,
        vy: 0
      };
      nodeMap.set(vid, node);
      graphNodes.push(node);
    }
  }

  const statGraph = document.getElementById('stat-graph');
  if (statGraph) {
    if (capped) {
      statGraph.textContent = `${graphNodes.length} nodes (capped at 250 for 60 FPS), ${graphEdges.length} edges | Full data in Table view`;
    } else {
      statGraph.textContent = `${graphNodes.length} nodes, ${graphEdges.length} edges`;
    }
  }
}
