pub const HTML_INDEX: &str = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>GDB Studio — Distributed Graph Database Workspace</title>
  <style>
    :root {
      --bg-darker: #0d1117;
      --bg-dark: #161b22;
      --bg-card: #21262d;
      --bg-input: #0b0e14;
      --border: #30363d;
      --border-focus: #58a6ff;
      --text: #c9d1d9;
      --text-muted: #8b949e;
      --text-bright: #f0f6fc;
      --accent: #58a6ff;
      --accent-glow: rgba(88, 166, 255, 0.25);
      --success: #3fb950;
      --warning: #d29922;
      --danger: #f85149;
      --purple: #bc8cff;
      --cyan: #39c5bb;
      --orange: #f0883e;
      --node-color-0: #58a6ff;
      --node-color-1: #bc8cff;
      --node-color-2: #3fb950;
      --node-color-3: #f0883e;
      --node-color-4: #39c5bb;
      --node-color-5: #e36209;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
    body { background-color: var(--bg-darker); color: var(--text); height: 100vh; display: flex; flex-direction: column; overflow: hidden; }

    /* Top Navigation Bar */
    header {
      background: var(--bg-dark);
      border-bottom: 1px solid var(--border);
      height: 52px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 16px;
      flex-shrink: 0;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 10px;
      font-weight: 700;
      font-size: 16px;
      color: var(--text-bright);
      letter-spacing: 0.5px;
    }
    .brand-badge {
      background: linear-gradient(135deg, #238636, #2ea043);
      color: #fff;
      font-size: 10px;
      padding: 2px 6px;
      border-radius: 10px;
      font-weight: 600;
      text-transform: uppercase;
    }
    .cluster-controls {
      display: flex;
      align-items: center;
      gap: 12px;
    }
    .cluster-input-group {
      display: flex;
      align-items: center;
      background: var(--bg-input);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 4px 8px;
      font-size: 12px;
    }
    .cluster-input-group label { color: var(--text-muted); margin-right: 6px; font-size: 11px; }
    .cluster-input-group input {
      background: transparent;
      border: none;
      color: var(--text-bright);
      font-family: monospace;
      outline: none;
      width: 170px;
      font-size: 12px;
    }
    .status-indicator {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
      font-weight: 500;
      padding: 4px 10px;
      border-radius: 12px;
      background: rgba(46, 160, 67, 0.15);
      color: var(--success);
      border: 1px solid rgba(46, 160, 67, 0.3);
    }
    .status-indicator.offline {
      background: rgba(248, 81, 73, 0.15);
      color: var(--danger);
      border-color: rgba(248, 81, 73, 0.3);
    }
    .status-dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
    .node-pills { display: flex; gap: 6px; }
    .node-pill {
      font-size: 11px;
      background: var(--bg-card);
      border: 1px solid var(--border);
      padding: 2px 8px;
      border-radius: 4px;
      color: var(--text-muted);
    }
    .node-pill.active { color: var(--text-bright); border-color: var(--accent); }

    /* Main Workspace Layout */
    .workspace {
      display: flex;
      flex: 1;
      height: calc(100vh - 52px - 28px);
      overflow: hidden;
    }

    /* Left Sidebar */
    .sidebar {
      width: 280px;
      background: var(--bg-dark);
      border-right: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      flex-shrink: 0;
    }
    .sidebar-tabs {
      display: flex;
      border-bottom: 1px solid var(--border);
      background: var(--bg-darker);
    }
    .sidebar-tab {
      flex: 1;
      text-align: center;
      padding: 10px 4px;
      font-size: 12px;
      font-weight: 600;
      color: var(--text-muted);
      cursor: pointer;
      border-bottom: 2px solid transparent;
      transition: all 0.2s;
    }
    .sidebar-tab.active {
      color: var(--accent);
      border-bottom-color: var(--accent);
      background: var(--bg-dark);
    }
    .sidebar-content {
      flex: 1;
      overflow-y: auto;
      padding: 12px;
    }
    .section-title {
      font-size: 11px;
      text-transform: uppercase;
      font-weight: 700;
      color: var(--text-muted);
      letter-spacing: 0.5px;
      margin-bottom: 8px;
    }
    .template-item, .history-item {
      padding: 8px 10px;
      margin-bottom: 6px;
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 6px;
      cursor: pointer;
      font-size: 12px;
      transition: all 0.15s;
    }
    .template-item:hover, .history-item:hover {
      border-color: var(--accent);
      background: #282f3a;
    }
    .template-title { font-weight: 600; color: var(--text-bright); margin-bottom: 3px; }
    .template-query { font-family: monospace; font-size: 11px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .schema-tag {
      display: inline-block;
      padding: 3px 8px;
      border-radius: 4px;
      font-size: 11px;
      font-family: monospace;
      margin: 2px 4px 6px 0;
      background: rgba(88, 166, 255, 0.15);
      color: var(--accent);
      border: 1px solid rgba(88, 166, 255, 0.3);
      cursor: pointer;
    }
    .schema-tag.edge {
      background: rgba(188, 140, 255, 0.15);
      color: var(--purple);
      border-color: rgba(188, 140, 255, 0.3);
    }

    /* Central Content Area */
    .content-area {
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      background: var(--bg-darker);
    }

    /* Top: Query Editor */
    .editor-pane {
      height: 180px;
      min-height: 120px;
      background: var(--bg-dark);
      border-bottom: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      flex-shrink: 0;
    }
    .editor-toolbar {
      height: 36px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 12px;
      background: #141820;
      border-bottom: 1px solid var(--border);
    }
    .editor-title { font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; }
    .editor-actions { display: flex; align-items: center; gap: 8px; }
    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 5px 12px;
      border-radius: 6px;
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
      border: 1px solid transparent;
      transition: all 0.15s;
    }
    .btn-primary {
      background: #238636;
      color: #fff;
    }
    .btn-primary:hover { background: #2ea043; }
    .btn-secondary {
      background: var(--bg-card);
      border-color: var(--border);
      color: var(--text);
    }
    .btn-secondary:hover { background: #2e3440; border-color: #8b949e; }
    .btn-sm { padding: 3px 8px; font-size: 11px; }

    .editor-container {
      flex: 1;
      position: relative;
    }
    #query-input {
      width: 100%;
      height: 100%;
      background: var(--bg-input);
      border: none;
      color: #79c0ff;
      font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
      font-size: 14px;
      line-height: 1.5;
      padding: 12px;
      resize: none;
      outline: none;
    }

    /* Bottom: Results Viewports */
    .results-pane {
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      position: relative;
    }
    .results-nav {
      height: 38px;
      background: var(--bg-dark);
      border-bottom: 1px solid var(--border);
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 12px;
      flex-shrink: 0;
    }
    .nav-tabs { display: flex; gap: 2px; }
    .nav-tab {
      padding: 6px 14px;
      font-size: 12px;
      font-weight: 600;
      color: var(--text-muted);
      cursor: pointer;
      border-radius: 6px;
      transition: all 0.15s;
      display: flex;
      align-items: center;
      gap: 6px;
    }
    .nav-tab.active {
      color: var(--text-bright);
      background: var(--bg-card);
    }
    .nav-stats {
      font-size: 11px;
      font-family: monospace;
      color: var(--text-muted);
      display: flex;
      gap: 12px;
    }
    .nav-stats span.highlight { color: var(--cyan); font-weight: 600; }

    .viewport { flex: 1; position: relative; overflow: hidden; display: none; }
    .viewport.active { display: flex; }

    /* Graph Canvas Viewport */
    #graph-viewport {
      background: radial-gradient(circle at center, #151b23 0%, #0d1117 100%);
      width: 100%;
      height: 100%;
      position: relative;
    }
    #graph-canvas {
      width: 100%;
      height: 100%;
      display: block;
      cursor: grab;
    }
    #graph-canvas:active { cursor: grabbing; }

    .graph-controls {
      position: absolute;
      bottom: 16px;
      left: 16px;
      display: flex;
      gap: 6px;
      background: rgba(22, 27, 34, 0.85);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 4px;
      backdrop-filter: blur(8px);
      z-index: 10;
    }
    .control-btn {
      background: transparent;
      border: none;
      color: var(--text);
      width: 28px;
      height: 28px;
      border-radius: 4px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 14px;
    }
    .control-btn:hover { background: var(--bg-card); color: var(--accent); }

    /* Table Viewport */
    #table-viewport {
      padding: 12px;
      overflow: auto;
      width: 100%;
      height: 100%;
      background: var(--bg-darker);
    }
    table.data-grid {
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
      font-family: monospace;
    }
    table.data-grid th {
      background: var(--bg-dark);
      color: var(--text-bright);
      text-align: left;
      padding: 8px 12px;
      border-bottom: 2px solid var(--border);
      position: sticky;
      top: 0;
      z-index: 2;
    }
    table.data-grid td {
      padding: 8px 12px;
      border-bottom: 1px solid var(--border);
      color: var(--text);
    }
    table.data-grid tr:hover td { background: rgba(88, 166, 255, 0.05); }

    /* JSON Viewport */
    #json-viewport {
      padding: 16px;
      overflow: auto;
      width: 100%;
      height: 100%;
      background: var(--bg-input);
    }
    #json-output {
      font-family: monospace;
      font-size: 12px;
      color: #79c0ff;
      white-space: pre-wrap;
    }

    /* Right Side: Property Inspector */
    .inspector {
      width: 260px;
      background: var(--bg-dark);
      border-left: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      flex-shrink: 0;
      transition: width 0.2s;
    }
    .inspector.closed { width: 0; display: none; }
    .inspector-header {
      padding: 12px;
      border-bottom: 1px solid var(--border);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .inspector-title { font-size: 12px; font-weight: 700; color: var(--text-bright); }
    .inspector-body { flex: 1; padding: 12px; overflow-y: auto; font-size: 12px; }
    .prop-row {
      display: flex;
      flex-direction: column;
      margin-bottom: 10px;
      padding-bottom: 8px;
      border-bottom: 1px solid rgba(48, 54, 61, 0.5);
    }
    .prop-key { color: var(--text-muted); font-size: 10px; text-transform: uppercase; font-weight: 600; margin-bottom: 2px; }
    .prop-val { color: var(--text-bright); font-family: monospace; word-break: break-all; }

    /* Footer */
    footer {
      height: 28px;
      background: var(--bg-dark);
      border-top: 1px solid var(--border);
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 16px;
      font-size: 11px;
      color: var(--text-muted);
      flex-shrink: 0;
    }
    .footer-left { display: flex; gap: 16px; }
    .footer-right { display: flex; gap: 12px; }

    /* Loading overlay */
    .spinner {
      display: inline-block;
      width: 12px;
      height: 12px;
      border: 2px solid rgba(255,255,255,0.3);
      border-radius: 50%;
      border-top-color: #fff;
      animation: spin 0.8s ease-in-out infinite;
    }
    /* Cluster & Resources Viewports */
    .dashboard-container {
      width: 100%;
      height: 100%;
      overflow-y: auto;
      padding: 24px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }
    .dash-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      border-bottom: 1px solid var(--border);
      padding-bottom: 14px;
    }
    .dash-title {
      font-size: 16px;
      font-weight: 700;
      color: var(--text-bright);
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .dash-badge {
      font-size: 11px;
      padding: 3px 8px;
      border-radius: 12px;
      font-weight: 600;
    }
    .dash-badge.green { background: rgba(46, 160, 67, 0.15); color: var(--success); border: 1px solid rgba(46, 160, 67, 0.3); }
    .dash-badge.purple { background: rgba(188, 140, 255, 0.15); color: var(--purple); border: 1px solid rgba(188, 140, 255, 0.3); }
    .dash-badge.orange { background: rgba(240, 136, 62, 0.15); color: var(--orange); border: 1px solid rgba(240, 136, 62, 0.3); }
    .dash-badge.blue { background: rgba(88, 166, 255, 0.15); color: var(--accent); border: 1px solid rgba(88, 166, 255, 0.3); }
    .dash-badge.gray { background: rgba(139, 148, 158, 0.15); color: var(--text-muted); border: 1px solid rgba(139, 148, 158, 0.3); }

    .dash-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 16px;
    }
    .dash-card {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .card-label {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--text-muted);
    }
    .card-value {
      font-size: 22px;
      font-weight: 700;
      color: var(--text-bright);
      font-family: ui-monospace, SFMono-Regular, monospace;
    }
    .card-subtext {
      font-size: 11px;
      color: var(--text-muted);
    }
    .dash-section {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .dash-section-title {
      font-size: 13px;
      font-weight: 700;
      color: var(--text-bright);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .node-action-btn {
      background: rgba(88, 166, 255, 0.1);
      border: 1px solid rgba(88, 166, 255, 0.3);
      color: var(--accent);
      border-radius: 4px;
      padding: 2px 8px;
      font-size: 11px;
      cursor: pointer;
    }
    .node-action-btn:hover {
      background: rgba(88, 166, 255, 0.25);
    }

    /* Schema Manager & Modals */
    .schema-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
      gap: 16px;
    }
    .schema-card {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .schema-card-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 8px;
      border-bottom: 1px solid var(--border);
    }
    .schema-prop-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 11px;
    }
    .schema-prop-table th {
      text-align: left;
      padding: 6px 8px;
      color: var(--text-muted);
      border-bottom: 1px solid var(--border);
      font-weight: 600;
    }
    .schema-prop-table td {
      padding: 6px 8px;
      border-bottom: 1px solid rgba(48, 54, 61, 0.4);
      color: var(--text);
    }
    .schema-prop-table tr:last-child td {
      border-bottom: none;
    }
    .prop-type-badge {
      font-family: monospace;
      font-size: 10px;
      background: rgba(88, 166, 255, 0.12);
      color: var(--accent);
      padding: 2px 6px;
      border-radius: 4px;
      border: 1px solid rgba(88, 166, 255, 0.25);
    }
    .index-badge {
      font-size: 9px;
      font-weight: 700;
      background: rgba(63, 185, 80, 0.15);
      color: var(--green);
      padding: 1px 5px;
      border-radius: 4px;
      border: 1px solid rgba(63, 185, 80, 0.3);
      display: inline-flex;
      align-items: center;
      gap: 3px;
    }

    .modal-overlay {
      position: fixed;
      top: 0;
      left: 0;
      width: 100vw;
      height: 100vh;
      background: rgba(0, 0, 0, 0.7);
      backdrop-filter: blur(4px);
      z-index: 1000;
      display: none;
      align-items: center;
      justify-content: center;
    }
    .modal-box {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 10px;
      width: 500px;
      max-width: 90vw;
      padding: 20px;
      box-shadow: 0 16px 36px rgba(0, 0, 0, 0.5);
      display: flex;
      flex-direction: column;
      gap: 14px;
    }
    .modal-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      font-weight: 700;
      font-size: 14px;
      color: var(--text-bright);
    }
    .modal-body {
      display: flex;
      flex-direction: column;
      gap: 12px;
      font-size: 12px;
    }
    .modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
      margin-top: 8px;
    }
    .form-group {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .form-group label {
      font-size: 11px;
      color: var(--text-muted);
      font-weight: 600;
    }
    .form-input {
      background: var(--bg-dark);
      border: 1px solid var(--border);
      border-radius: 6px;
      color: var(--text);
      padding: 6px 10px;
      font-size: 12px;
      outline: none;
    }
    .form-input:focus {
      border-color: var(--accent);
    }
  </style>
</head>
<body>

  <!-- Top Navigation -->
  <header>
    <div class="brand">
      <span>⚡ GDB STUDIO</span>
      <span class="brand-badge">v0.4.0</span>
    </div>

    <div class="cluster-controls">
      <div class="cluster-input-group">
        <label>ENDPOINT:</label>
        <input type="text" id="cluster-url" value="http://localhost:8847" placeholder="http://host:port">
      </div>
      <div id="cluster-status-pill" class="status-indicator">
        <div class="status-dot"></div>
        <span id="cluster-status-text">Connected</span>
      </div>
      <div class="node-pills" id="header-node-pills">
        <div class="node-pill active" title="Flight :8848 | HTTP :8847">Peer #1</div>
      </div>
    </div>
  </header>

  <!-- Workspace -->
  <div class="workspace">

    <!-- Left Sidebar: Templates, Catalog, Cluster, History -->
    <div class="sidebar">
      <div class="sidebar-tabs">
        <div class="sidebar-tab active" onclick="switchSidebarTab('templates', this)">Templates</div>
        <div class="sidebar-tab" onclick="switchSidebarTab('schema', this)">Schema</div>
        <div class="sidebar-tab" onclick="switchSidebarTab('cluster', this)">Cluster</div>
        <div class="sidebar-tab" onclick="switchSidebarTab('history', this)">History</div>
      </div>

      <div class="sidebar-content">
        <!-- Templates View -->
        <div id="tab-templates">
          <div class="section-title">Pattern Matching</div>
          <div class="template-item" onclick="setQuery('MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name LIMIT 50;')">
            <div class="template-title">1-Hop Traversal</div>
            <div class="template-query">MATCH (a)-[:FOLLOWS]->(b)...</div>
          </div>
          <div class="template-item" onclick="setQuery('MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User) RETURN a.name, b.name, c.name LIMIT 25;')">
            <div class="template-title">2-Hop Traversal</div>
            <div class="template-query">MATCH (a)-[:FOLLOWS]->(b)-[:FOLLOWS]->(c)...</div>
          </div>

          <div class="section-title" style="margin-top: 14px;">Graph Analytics</div>
          <div class="template-item" onclick="setQuery('CALL algo.pageRank() YIELD vertex_id, score;')">
            <div class="template-title">PageRank</div>
            <div class="template-query">CALL algo.pageRank()...</div>
          </div>
          <div class="template-item" onclick="setQuery('CALL algo.louvain() YIELD vertex_id, community;')">
            <div class="template-title">Louvain Communities</div>
            <div class="template-query">CALL algo.louvain()...</div>
          </div>
          <div class="template-item" onclick="setQuery('CALL algo.wcc() YIELD vertex_id, component;')">
            <div class="template-title">Weakly Connected Components</div>
            <div class="template-query">CALL algo.wcc()...</div>
          </div>
          <div class="template-item" onclick="setQuery('CALL algo.triangleCount() YIELD vertex_id, triangles;')">
            <div class="template-title">Triangle Count & LCC</div>
            <div class="template-query">CALL algo.triangleCount()...</div>
          </div>
          <div class="template-item" onclick="setQuery('CALL algo.sssp(1) YIELD vertex_id, distance;')">
            <div class="template-title">Single Source Shortest Path</div>
            <div class="template-query">CALL algo.sssp(1)...</div>
          </div>

          <div class="section-title" style="margin-top: 14px;">DDL / Operations</div>
          <div class="template-item" onclick="setQuery('CREATE VERTEX User (name STRING, age INT64);')">
            <div class="template-title">Create Vertex Schema</div>
            <div class="template-query">CREATE VERTEX User...</div>
          </div>
          <div class="template-item" onclick="setQuery('compact;')">
            <div class="template-title">Force CSR Compaction</div>
            <div class="template-query">compact;</div>
          </div>
        </div>

        <!-- Schema View -->
        <div id="tab-schema" style="display:none;">
          <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:12px;">
            <div class="section-title" style="margin-bottom:0;">Schema Catalog</div>
            <button class="btn btn-secondary btn-sm" style="font-size:10px; padding:2px 8px;" onclick="switchView('schema', document.getElementById('tab-btn-schema'))">📐 Manager</button>
          </div>
          <div class="section-title">Vertex Tags</div>
          <div id="schema-vertices">
            <div style="font-size:11px; color:var(--text-muted);">Loading tags...</div>
          </div>
          <div class="section-title" style="margin-top: 14px;">Edge Types</div>
          <div id="schema-edges">
            <div style="font-size:11px; color:var(--text-muted);">Loading edge types...</div>
          </div>
        </div>

        <!-- Cluster View -->
        <div id="tab-cluster" style="display:none;">
          <div class="section-title">Ring Peer Nodes</div>
          <div id="sidebar-cluster-nodes">
            <div style="font-size:11px; color:var(--text-muted);">Loading cluster peers...</div>
          </div>
          <div class="section-title" style="margin-top: 14px;">Quick Actions</div>
          <div class="template-item" onclick="switchView('cluster', document.getElementById('tab-btn-cluster'))">
            <div class="template-title">Open Cluster Ring Dashboard</div>
            <div class="template-query">View full topology &amp; tokens</div>
          </div>
          <div class="template-item" onclick="switchView('resources', document.getElementById('tab-btn-resources'))">
            <div class="template-title">Open Storage &amp; Resources</div>
            <div class="template-query">Inspect RAM, CSR, Compactions</div>
          </div>
        </div>

        <!-- History View -->
        <div id="tab-history" style="display:none;">
          <div class="section-title">Recent Queries</div>
          <div id="history-list">
            <div style="font-size:11px; color:var(--text-muted);">No queries executed yet.</div>
          </div>
        </div>
      </div>
    </div>

    <!-- Center Content: Query Editor + Viewports -->
    <div class="content-area">
      <!-- Editor -->
      <div class="editor-pane">
        <div class="editor-toolbar">
          <div class="editor-title">Query Editor (openCypher / GQL)</div>
          <div class="editor-actions">
            <button class="btn btn-secondary btn-sm" onclick="clearQuery()">Clear</button>
            <button class="btn btn-secondary btn-sm" id="explain-btn" onclick="explainQuery()">🔍 Explain Plan</button>
            <button class="btn btn-primary btn-sm" id="run-btn" onclick="executeQuery()">
              <span id="run-spinner" style="display:none;" class="spinner"></span>
              <span>▶ Run (Cmd+↵)</span>
            </button>
          </div>
        </div>
        <div class="editor-container">
          <textarea id="query-input" spellcheck="false" placeholder="Enter openCypher, GQL, or CALL algo query here...">SHOW SCHEMA;</textarea>
        </div>
      </div>

      <!-- Results Viewports -->
      <div class="results-pane">
        <div class="results-nav">
          <div class="nav-tabs">
            <div class="nav-tab active" id="tab-btn-graph" onclick="switchView('graph', this)">🕸️ Graph View</div>
            <div class="nav-tab" id="tab-btn-table" onclick="switchView('table', this)">📊 Table View</div>
            <div class="nav-tab" id="tab-btn-plan" onclick="switchView('plan', this)">🔍 Plan / Explain</div>
            <div class="nav-tab" id="tab-btn-schema" onclick="switchView('schema', this)">📐 Schema Manager</div>
            <div class="nav-tab" id="tab-btn-cluster" onclick="switchView('cluster', this)">🌐 Cluster Ring</div>
            <div class="nav-tab" id="tab-btn-resources" onclick="switchView('resources', this)">⚡ Storage &amp; Resources</div>
            <div class="nav-tab" id="tab-btn-unity" onclick="switchView('unity', this)">🎮 3D Unity View</div>
            <div class="nav-tab" id="tab-btn-json" onclick="switchView('json', this)">📜 Raw JSON</div>
          </div>
          <div class="nav-stats" id="query-stats">
            <div>Status: <span class="highlight" id="stat-status">Ready</span></div>
            <div>Time: <span class="highlight" id="stat-time">0.0 ms</span></div>
            <div>Rows: <span class="highlight" id="stat-rows">0</span></div>
            <div>Graph: <span class="highlight" id="stat-graph">0 nodes, 0 edges</span></div>
          </div>
        </div>

        <!-- Graph Viewport -->
        <div id="graph-viewport" class="viewport active">
          <canvas id="graph-canvas"></canvas>
          <div class="graph-controls">
            <button class="control-btn" title="Zoom In" onclick="zoomGraph(1.2)">➕</button>
            <button class="control-btn" title="Zoom Out" onclick="zoomGraph(0.8)">➖</button>
            <button class="control-btn" title="Reset View" onclick="resetGraphView()">⟲</button>
            <button class="control-btn" title="Toggle Physics" id="physics-btn" onclick="togglePhysics()">⏸</button>
            <button class="control-btn" title="Export PNG" onclick="exportGraphPng()">📷 PNG</button>
            <button class="control-btn" title="Export SVG" onclick="exportGraphSvg()">🖼️ SVG</button>
            <button class="control-btn" title="Export CSV" onclick="exportGraphCsv()">📊 CSV</button>
            <button class="control-btn" title="Export JSON" onclick="exportGraphJson()">📋 JSON</button>
          </div>
        </div>

        <!-- Table Viewport -->
        <div id="table-viewport" class="viewport">
          <table class="data-grid" id="data-table">
            <thead><tr id="table-header"><th>No Data</th></tr></thead>
            <tbody id="table-body"><tr><td>Execute a query to view tabular results</td></tr></tbody>
          </table>
        </div>

        <!-- Plan / Explain Viewport -->
        <div id="plan-viewport" class="viewport" style="display:none; padding:20px; overflow-y:auto; background:var(--bg-darker);">
          <div style="max-width:960px; margin:0 auto;">
            <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:16px;">
              <div style="display:flex; align-items:center; gap:10px;">
                <span style="font-size:16px; font-weight:700; color:var(--text-bright);">🔍 Query Physical Execution Plan</span>
                <span id="plan-badge" class="dash-badge blue">Cost-Based Optimization</span>
              </div>
              <button class="btn btn-secondary btn-sm" onclick="copyPlanText()">📋 Copy Plan Text</button>
            </div>
            <div id="plan-dag-container" style="display:flex; flex-direction:column; gap:8px; margin-bottom:20px;"></div>
            <div class="dash-card" style="background:var(--bg-dark); border:1px solid var(--border); border-radius:8px; padding:16px;">
              <div class="card-label" style="margin-bottom:8px;">Plan Hierarchy (ASCII Tree)</div>
              <pre id="plan-ascii-output" style="font-family:monospace; font-size:12px; color:var(--text); line-height:1.5; overflow-x:auto; margin:0;">Execute EXPLAIN &lt;query&gt; to generate execution plan DAG.</pre>
            </div>
          </div>
        </div>

        <!-- Cluster Ring Viewport -->
        <div id="cluster-viewport" class="viewport">
          <div class="dashboard-container">
            <div class="dash-header">
              <div class="dash-title">
                <span>🌐 Leaderless Ring Cluster Topology</span>
                <span id="cluster-rf-badge" class="dash-badge green">RF = 3 (SYNC)</span>
                <span id="cluster-gpu-badge" class="dash-badge purple">GPU Disabled</span>
              </div>
              <div style="display:flex; gap:8px;">
                <button class="btn btn-secondary btn-sm" onclick="fetchClusterStatus()">🔄 Refresh Cluster</button>
              </div>
            </div>

            <div class="dash-grid">
              <div class="dash-card">
                <div class="card-label">Topology Architecture</div>
                <div class="card-value" style="font-size:16px; color:var(--accent);">LEADERLESS HASH RING</div>
                <div class="card-subtext" id="cluster-sub-topology">Consistent hashing across peer nodes</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Replication Factor</div>
                <div class="card-value" id="cluster-rf-val">RF = 3</div>
                <div class="card-subtext" id="cluster-rf-sub">Mode: Synchronous Quorum</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Active Ring Nodes</div>
                <div class="card-value" id="cluster-nodes-count">3 Peers</div>
                <div class="card-subtext" id="cluster-partitions-count">8 Partitions per Node</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Hardware Acceleration</div>
                <div class="card-value" id="cluster-gpu-val" style="font-size:16px;">Disabled</div>
                <div class="card-subtext" id="cluster-gpu-sub">Pass --enable-gpu to activate</div>
              </div>
            </div>

            <div class="dash-section">
              <div class="dash-section-title">
                <span>Ring Member Nodes</span>
                <span style="font-size:11px; font-weight:normal; color:var(--text-muted);" id="cluster-last-updated">Updated: Live</span>
              </div>
              <table class="data-grid" style="width:100%;">
                <thead>
                  <tr>
                    <th>Node ID</th>
                    <th>Role</th>
                    <th>REST HTTP Endpoint</th>
                    <th>Internal Flight Port</th>
                    <th>Client Flight Port</th>
                    <th>Assigned Token Range</th>
                    <th>Status</th>
                    <th>Action</th>
                  </tr>
                </thead>
                <tbody id="cluster-nodes-table-body">
                  <tr><td colspan="8" style="text-align:center;">Loading cluster nodes...</td></tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>

        <!-- Resources & Storage Viewport -->
        <div id="resources-viewport" class="viewport">
          <div class="dashboard-container">
            <div class="dash-header">
              <div class="dash-title">
                <span>⚡ System Resources &amp; In-Memory Storage</span>
                <span id="res-health-badge" class="dash-badge green">Storage Healthy</span>
              </div>
              <div style="display:flex; gap:8px;">
                <button class="btn btn-primary btn-sm" id="btn-trigger-compact" onclick="triggerCompaction()">⚡ Compact CSR Now</button>
                <button class="btn btn-secondary btn-sm" onclick="fetchResources()">🔄 Refresh</button>
              </div>
            </div>

            <div class="dash-grid">
              <div class="dash-card">
                <div class="card-label">In-Memory Allocated RAM</div>
                <div class="card-value" id="res-ram-val">0.00 MB</div>
                <div class="card-subtext" id="res-ram-sub">CSR structure + Delta write buffers</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Total Vertices (CSR)</div>
                <div class="card-value" id="res-v-val">0</div>
                <div class="card-subtext">Compacted vertex index</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Total Graph Edges</div>
                <div class="card-value" id="res-e-val">0</div>
                <div class="card-subtext" id="res-edges-breakdown">CSR: 0 | Delta: 0</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Background Compactions</div>
                <div class="card-value" id="res-compaction-val">0</div>
                <div class="card-subtext">Delta to Chunked-CSR merges</div>
              </div>
            </div>

            <div class="dash-grid">
              <div class="dash-card" style="grid-column: span 2;">
                <div class="card-label">GPU Acceleration &amp; Offload Engine</div>
                <div style="display:flex; justify-content:space-between; align-items:center; margin-top:4px;">
                  <div>
                    <div class="card-value" id="res-gpu-name" style="font-size:18px;">Apple Metal UMA</div>
                    <div class="card-subtext" id="res-gpu-device" style="margin-top:2px;">Device #0 | Offload Threshold: 10,000 edges</div>
                  </div>
                  <span id="res-gpu-status-badge" class="dash-badge gray">Disabled</span>
                </div>
                <div style="font-size:11px; color:var(--text-muted); margin-top:8px;">
                  Supported kernels: BFS Frontier Expansion, Vectorized PageRank, SIMD Graph Kernel
                </div>
              </div>
              <div class="dash-card">
                <div class="card-label">Cloud Tiered Storage (S3)</div>
                <div class="card-value" id="res-s3-val" style="font-size:18px;">Disabled</div>
                <div class="card-subtext" id="res-s3-sub">Local WAL &amp; memory persistence only</div>
              </div>
              <div class="dash-card">
                <div class="card-label">Executed Query Statistics</div>
                <div class="card-value" id="res-queries-val">0</div>
                <div class="card-subtext" id="res-queries-sub">OK: 0 | Errors: 0</div>
              </div>
            </div>

            <!-- Live Telemetry Sparklines -->
            <div class="dash-section">
              <div class="dash-section-title">
                <span>📈 Real-Time Engine Telemetry</span>
                <span style="font-size:11px; font-weight:normal; color:var(--text-muted);" id="telemetry-live-badge">🟢 Polling Live</span>
              </div>
              <div class="dash-grid" style="grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));">
                <div class="dash-card">
                  <div style="display:flex; justify-content:space-between; align-items:center;">
                    <div class="card-label">Throughput (QPS)</div>
                    <div id="spark-qps-val" style="font-size:14px; font-weight:700; color:var(--accent); font-family:monospace;">0.0 /s</div>
                  </div>
                  <svg id="sparkline-qps" width="100%" height="48" style="overflow:visible; margin-top:8px;"></svg>
                </div>
                <div class="dash-card">
                  <div style="display:flex; justify-content:space-between; align-items:center;">
                    <div class="card-label">Execution Latency</div>
                    <div id="spark-lat-val" style="font-size:14px; font-weight:700; color:var(--success); font-family:monospace;">0.0 ms</div>
                  </div>
                  <svg id="sparkline-latency" width="100%" height="48" style="overflow:visible; margin-top:8px;"></svg>
                </div>
                <div class="dash-card">
                  <div style="display:flex; justify-content:space-between; align-items:center;">
                    <div class="card-label">Allocated RAM Trend</div>
                    <div id="spark-ram-val" style="font-size:14px; font-weight:700; color:var(--purple); font-family:monospace;">0.0 MB</div>
                  </div>
                  <svg id="sparkline-ram" width="100%" height="48" style="overflow:visible; margin-top:8px;"></svg>
                </div>
              </div>
            </div>

            <div id="compaction-alert" style="display:none; padding:10px 14px; border-radius:6px; font-size:12px; background:rgba(63,185,80,0.15); border:1px solid #238636; color:#3fb950;">
              Compaction completed successfully!
            </div>
          </div>
        </div>

        <!-- Schema Manager Viewport -->
        <div id="schema-viewport" class="viewport" style="display:none; padding:20px; overflow-y:auto; background:var(--bg-darker);">
          <div class="dashboard-container" style="max-width:1200px; margin:0 auto;">
            <div class="dash-header">
              <div class="dash-title">
                <span>📐 Database Schema Catalog &amp; DDL Manager</span>
                <span id="schema-total-tags-badge" class="dash-badge blue">0 Vertex Tags</span>
                <span id="schema-total-edges-badge" class="dash-badge purple">0 Edge Types</span>
              </div>
              <div style="display:flex; gap:8px;">
                <button class="btn btn-primary btn-sm" onclick="openCreateVertexModal()">➕ Create Vertex Tag</button>
                <button class="btn btn-primary btn-sm" style="background:#8957e5;" onclick="openCreateEdgeModal()">➕ Create Edge Type</button>
                <button class="btn btn-secondary btn-sm" onclick="fetchSchema()">🔄 Refresh Schema</button>
              </div>
            </div>

            <!-- Vertex Tags Section -->
            <div class="dash-section" style="margin-bottom:24px;">
              <div class="dash-section-title" style="display:flex; justify-content:space-between; align-items:center;">
                <span>Vertex Tags (Entity Schemas)</span>
                <span style="font-size:11px; font-weight:normal; color:var(--text-muted);" id="schema-v-count-label">0 tags</span>
              </div>
              <div id="schema-vertex-grid" class="schema-grid">
                <div style="grid-column: 1/-1; padding:24px; text-align:center; color:var(--text-muted); background:var(--bg-card); border-radius:8px; border:1px dashed var(--border);">
                  No vertex schemas defined yet. Create your first vertex tag or execute <code>CREATE VERTEX TagName (id: INT64, ...);</code>
                </div>
              </div>
            </div>

            <!-- Edge Types Section -->
            <div class="dash-section">
              <div class="dash-section-title" style="display:flex; justify-content:space-between; align-items:center;">
                <span>Edge Types (Relationship Schemas)</span>
                <span style="font-size:11px; font-weight:normal; color:var(--text-muted);" id="schema-e-count-label">0 edge types</span>
              </div>
              <div id="schema-edge-grid" class="schema-grid">
                <div style="grid-column: 1/-1; padding:24px; text-align:center; color:var(--text-muted); background:var(--bg-card); border-radius:8px; border:1px dashed var(--border);">
                  No edge schemas defined yet. Create your first edge type or execute <code>CREATE EDGE EdgeType (weight: FLOAT64, ...);</code>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 3D Unity Viewport -->
        <div id="unity-viewport" class="viewport" style="display:none; width:100%; height:100%; position:relative; background:#080b10; overflow:hidden;">
          <canvas id="unity-canvas" style="width:100%; height:100%; display:block; outline:none; cursor:grab;"></canvas>
          <div class="graph-controls" style="top:14px; right:14px;">
            <button class="control-btn" title="Zoom In" onclick="zoomUnity3D(1.2)">➕</button>
            <button class="control-btn" title="Zoom Out" onclick="zoomUnity3D(0.8)">➖</button>
            <button class="control-btn" title="Reset 3D View" onclick="resetUnity3DView()">⟲</button>
            <button class="control-btn" title="Toggle 3D Physics" id="unity-physics-btn" onclick="toggleUnityPhysics()">⏸</button>
            <button class="control-btn" title="Auto Rotate" id="unity-rotate-btn" onclick="toggleUnityAutoRotate()">🔄</button>
          </div>
          <div id="unity-hud-bar" style="position:absolute; bottom:16px; left:16px; display:flex; align-items:center; gap:10px; background:rgba(13,17,23,0.85); backdrop-filter:blur(6px); border:1px solid var(--border); padding:8px 14px; border-radius:8px; font-size:12px; pointer-events:auto; z-index:10;">
            <span style="font-size:16px;">🎮</span>
            <div>
              <div style="font-weight:600; color:var(--text-bright);" id="unity-stats-title">3D Graph Spatial Visualizer</div>
              <div style="font-size:11px; color:var(--text-muted);" id="unity-status-badge">Drag to Orbit | Scroll to Zoom | Click Sphere to Inspect</div>
            </div>
          </div>
        </div>

        <!-- JSON Viewport -->
        <div id="json-viewport" class="viewport">
          <pre id="json-output">// Result JSON will appear here...</pre>
        </div>
      </div>
    </div>

    <!-- Right Side: Property Inspector -->
    <div class="inspector" id="inspector-panel">
      <div class="inspector-header">
        <div class="inspector-title" id="inspector-title">Entity Inspector</div>
        <button class="control-btn" style="width:20px;height:20px;" onclick="closeInspector()">✕</button>
      </div>
      <div class="inspector-body" id="inspector-body">
        <div style="color:var(--text-muted); font-size:11px;">Click on any node or edge in the graph to inspect its properties.</div>
      </div>
    </div>

  </div>

  <!-- Modal: Create Vertex Tag -->
  <div class="modal-overlay" id="modal-create-vertex">
    <div class="modal-box">
      <div class="modal-header">
        <span>➕ Create Vertex Tag</span>
        <button class="control-btn" style="width:20px;height:20px;" onclick="closeModal('modal-create-vertex')">✕</button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label>Vertex Tag Name</label>
          <input type="text" class="form-input" id="new-vertex-name" placeholder="e.g. Account, Customer, Product" />
        </div>
        <div class="form-group">
          <label>Primary Key Property (Optional)</label>
          <input type="text" class="form-input" id="new-vertex-pk" placeholder="e.g. id, uid, account_no" value="id" />
        </div>
        <div class="form-group">
          <label>Properties Definition (name: TYPE, separated by commas)</label>
          <input type="text" class="form-input" id="new-vertex-props" placeholder="e.g. name: STRING, age: INT64, balance: FLOAT64" value="name: STRING, created_at: INT64" />
          <div style="font-size:10px; color:var(--text-muted); margin-top:2px;">Supported types: STRING, INT64, FLOAT64, BOOLEAN</div>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary btn-sm" onclick="closeModal('modal-create-vertex')">Cancel</button>
        <button class="btn btn-primary btn-sm" onclick="submitCreateVertex()">Create Tag</button>
      </div>
    </div>
  </div>

  <!-- Modal: Create Edge Type -->
  <div class="modal-overlay" id="modal-create-edge">
    <div class="modal-box">
      <div class="modal-header">
        <span>➕ Create Edge Type</span>
        <button class="control-btn" style="width:20px;height:20px;" onclick="closeModal('modal-create-edge')">✕</button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label>Edge Type Name</label>
          <input type="text" class="form-input" id="new-edge-name" placeholder="e.g. TRANSFERRED_TO, LIKES, BELONGS_TO" />
        </div>
        <div class="form-group">
          <label>Properties Definition (Optional, name: TYPE, separated by commas)</label>
          <input type="text" class="form-input" id="new-edge-props" placeholder="e.g. amount: FLOAT64, timestamp: INT64" value="weight: FLOAT64, created_at: INT64" />
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary btn-sm" onclick="closeModal('modal-create-edge')">Cancel</button>
        <button class="btn btn-primary btn-sm" style="background:#8957e5;" onclick="submitCreateEdge()">Create Edge Type</button>
      </div>
    </div>
  </div>

  <!-- Modal: Add Property to Vertex/Edge -->
  <div class="modal-overlay" id="modal-add-prop">
    <div class="modal-box">
      <div class="modal-header">
        <span id="modal-add-prop-title">➕ Add Property</span>
        <button class="control-btn" style="width:20px;height:20px;" onclick="closeModal('modal-add-prop')">✕</button>
      </div>
      <div class="modal-body">
        <input type="hidden" id="add-prop-target-kind" value="vertex" />
        <input type="hidden" id="add-prop-target-name" value="" />
        <div class="form-group">
          <label>Target Schema</label>
          <input type="text" class="form-input" id="add-prop-target-display" disabled />
        </div>
        <div class="form-group">
          <label>Property Name</label>
          <input type="text" class="form-input" id="add-prop-name" placeholder="e.g. email, score, status" />
        </div>
        <div class="form-group">
          <label>Data Type</label>
          <select class="form-input" id="add-prop-type">
            <option value="STRING">STRING (Text)</option>
            <option value="INT64">INT64 (Integer)</option>
            <option value="FLOAT64">FLOAT64 (Float)</option>
            <option value="BOOLEAN">BOOLEAN (Bool)</option>
          </select>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary btn-sm" onclick="closeModal('modal-add-prop')">Cancel</button>
        <button class="btn btn-primary btn-sm" onclick="submitAddProperty()">Add Property</button>
      </div>
    </div>
  </div>

  <!-- Footer -->
  <footer>
    <div class="footer-left">
      <span id="footer-version">GDB Studio v0.4.1</span>
      <span id="footer-cluster-info">Cluster: Leaderless Ring (3 Peers)</span>
      <span id="footer-gpu-info">Acceleration: Metal / CUDA / CPU</span>
    </div>
    <div class="footer-right">
      <span id="footer-connection">Connected to http://localhost:8847</span>
    </div>
  </footer>

  <!-- Application Logic & Force-Directed Graph Engine -->
  <script>
    // State
    let queryHistory = [];
    let graphNodes = [];
    let graphEdges = [];
    let selectedEntity = null;
    let physicsRunning = true;

    // Canvas & Camera State
    const canvas = document.getElementById('graph-canvas');
    const ctx = canvas.getContext('2d');
    let width = canvas.clientWidth;
    let height = canvas.clientHeight;
    let cameraX = 0;
    let cameraY = 0;
    let cameraZoom = 1.0;
    let isDragging = false;
    let dragStart = { x: 0, y: 0 };
    let draggedNode = null;
    let hoveredNode = null;

    // Resize Canvas
    function resizeCanvas() {
      const rect = canvas.parentElement.getBoundingClientRect();
      width = rect.width;
      height = rect.height;
      canvas.width = width * window.devicePixelRatio;
      canvas.height = height * window.devicePixelRatio;
      ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
    }
    window.addEventListener('resize', resizeCanvas);
    setTimeout(resizeCanvas, 50);

    // Color Palette
    const COLORS = ['#58a6ff', '#bc8cff', '#3fb950', '#f0883e', '#39c5bb', '#f778ba', '#d29922', '#79c0ff'];
    function getNodeColor(label, id) {
      if (typeof id === 'number') return COLORS[id % COLORS.length];
      let hash = 0;
      const str = String(label || id);
      for (let i = 0; i < str.length; i++) hash = (hash << 5) - hash + str.charCodeAt(i);
      return COLORS[Math.abs(hash) % COLORS.length];
    }

    let currentView = 'graph';
    let lastLoadedGraphData = null;

    // Force Simulation
    function stepSimulation() {
      if (!physicsRunning || graphNodes.length === 0 || currentView !== 'graph') return;

      const repulsion = 4000;
      const springLength = 80;
      const springStrength = 0.05;
      const centerGravity = 0.02;

      // 1. Repulsion between nodes
      for (let i = 0; i < graphNodes.length; i++) {
        for (let j = i + 1; j < graphNodes.length; j++) {
          const a = graphNodes[i];
          const b = graphNodes[j];
          let dx = b.x - a.x;
          let dy = b.y - a.y;
          let dist = Math.sqrt(dx * dx + dy * dy) || 1;
          if (dist > 300) continue;
          let force = repulsion / (dist * dist);
          let fx = (dx / dist) * force;
          let fy = (dy / dist) * force;
          a.vx -= fx;
          a.vy -= fy;
          b.vx += fx;
          b.vy += fy;
        }
      }

      // 2. Attraction along edges
      for (const edge of graphEdges) {
        const a = edge.source;
        const b = edge.target;
        if (!a || !b) continue;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;
        let force = (dist - springLength) * springStrength;
        let fx = (dx / dist) * force;
        let fy = (dy / dist) * force;
        a.vx += fx;
        a.vy += fy;
        b.vx -= fx;
        b.vy -= fy;
      }

      // 3. Center gravity & update positions
      for (const n of graphNodes) {
        if (n === draggedNode) continue;
        n.vx -= n.x * centerGravity;
        n.vy -= n.y * centerGravity;
        n.vx *= 0.85; // Damping
        n.vy *= 0.85;
        n.x += n.vx;
        n.y += n.vy;
      }
    }

    // Render Loop
    function renderGraph() {
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

        ctx.strokeStyle = (selectedEntity === edge) ? '#58a6ff' : 'rgba(88, 166, 255, 0.35)';
        ctx.lineWidth = (selectedEntity === edge) ? 2.5 : 1.5;

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
          ctx.fillStyle = '#8b949e';
          ctx.font = '10px monospace';
          ctx.fillText(edge.label, midX + 4, midY - 4);
        }
      }

      // Draw Nodes
      for (const n of graphNodes) {
        const isHovered = (n === hoveredNode);
        const isSelected = (n === selectedEntity);
        const color = getNodeColor(n.label, n.id);

        // Glow halo on select/hover
        if (isSelected || isHovered) {
          ctx.beginPath();
          ctx.arc(n.x, n.y, 24, 0, Math.PI * 2);
          ctx.fillStyle = isSelected ? 'rgba(88, 166, 255, 0.3)' : 'rgba(255, 255, 255, 0.15)';
          ctx.fill();
        }

        // Node Circle
        ctx.beginPath();
        ctx.arc(n.x, n.y, 18, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.lineWidth = isSelected ? 3 : 1.5;
        ctx.strokeStyle = isSelected ? '#ffffff' : '#0d1117';
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

    function zoomGraph(factor) {
      cameraZoom = Math.max(0.2, Math.min(5.0, cameraZoom * factor));
    }
    function resetGraphView() {
      cameraX = 0; cameraY = 0; cameraZoom = 1.0;
    }
    function togglePhysics() {
      physicsRunning = !physicsRunning;
      document.getElementById('physics-btn').textContent = physicsRunning ? '⏸' : '▶';
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

    // Inspector
    function openInspector(entity) {
      const panel = document.getElementById('inspector-panel');
      const title = document.getElementById('inspector-title');
      const body = document.getElementById('inspector-body');
      panel.classList.remove('closed');

      title.textContent = `Node #${entity.id}`;
      let html = '';
      html += `<div class="prop-row"><div class="prop-key">ID</div><div class="prop-val">${entity.id}</div></div>`;
      if (entity.label) html += `<div class="prop-row"><div class="prop-key">Label / Tag</div><div class="prop-val">${entity.label}</div></div>`;

      if (entity.properties) {
        for (const [k, v] of Object.entries(entity.properties)) {
          html += `<div class="prop-row"><div class="prop-key">${k}</div><div class="prop-val">${JSON.stringify(v)}</div></div>`;
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
      document.getElementById('inspector-panel').classList.add('closed');
      selectedEntity = null;
    }

    // Query Execution
    async function executeQuery() {
      const query = document.getElementById('query-input').value.trim();
      const endpoint = document.getElementById('cluster-url').value.trim();
      if (!query) return;

      const runBtn = document.getElementById('run-btn');
      const spinner = document.getElementById('run-spinner');
      runBtn.disabled = true;
      spinner.style.display = 'inline-block';
      document.getElementById('stat-status').textContent = 'Executing...';

      const startTime = performance.now();

      try {
        const resp = await fetch('/api/query', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ query: query, endpoint: endpoint })
        });
        const data = await resp.json();
        const duration = (performance.now() - startTime).toFixed(1);

        document.getElementById('stat-status').textContent = data.status === 'ok' ? 'OK' : 'Error';
        document.getElementById('stat-time').textContent = `${(data.elapsed_us ? (data.elapsed_us / 1000).toFixed(2) : duration)} ms`;
        document.getElementById('stat-rows').textContent = data.num_rows || (data.rows ? data.rows.length : 0);

        const durationFloat = parseFloat(data.elapsed_us ? (data.elapsed_us / 1000).toFixed(2) : duration);
        if (typeof recordQueryLatency === 'function') {
          recordQueryLatency(durationFloat);
        }

        // Render JSON
        document.getElementById('json-output').textContent = JSON.stringify(data, null, 2);

        // Render Table
        renderTable(data);

        // Save data for Unity and Graph viewers
        lastLoadedGraphData = data;

        const queryLower = query.toLowerCase();
        const isAlgoQuery = queryLower.includes('call algo.') || queryLower.startsWith('call algo');
        const hasAlgoMetrics = data.columns && data.columns.some(c => 
          ['score', 'community', 'community_id', 'distance', 'triangles', 'jaccard', 'cosine', 'similarity', 'kcore'].includes(c.toLowerCase())
        );

        // Auto-switch view and manage graph physics
        if (data.plan || queryLower.startsWith('explain')) {
          renderPlan(data);
          switchView('plan', document.getElementById('tab-btn-plan'));
          document.getElementById('stat-graph').textContent = 'Physical plan DAG generated';
        } else if (isAlgoQuery || hasAlgoMetrics) {
          switchView('table', document.getElementById('tab-btn-table'));
          document.getElementById('stat-graph').textContent = 'Tabular algorithm output (Graph physics safely paused)';
        } else {
          // Extract and Render Graph
          if (data.graph && (data.graph.nodes.length > 0 || data.graph.edges.length > 0)) {
            buildGraph(data.graph.nodes, data.graph.edges);
          } else {
            extractGraphFromRows(data);
          }
        }

        if (currentView === 'unity') {
          renderUnityGraph();
        } else {
          sync3DGraph();
        }

        const isDdl = /^\s*(create|drop|alter)\s+(vertex|edge|tag|index)/i.test(query);
        if (isDdl && data.status === 'ok') {
          fetchSchema();
        }

        // Add to history
        addHistory(query, data.status === 'ok');

      } catch (err) {
        document.getElementById('stat-status').textContent = 'Failed';
        document.getElementById('json-output').textContent = `Request failed: ${err.message}`;
      } finally {
        runBtn.disabled = false;
        spinner.style.display = 'none';
      }
    }

    // Explain Query & Physical Execution Plan DAG Visualizer
    function explainQuery() {
      let q = document.getElementById('query-input').value.trim();
      if (!q) return;
      if (!q.toUpperCase().startsWith('EXPLAIN')) {
        q = 'EXPLAIN ' + q;
        document.getElementById('query-input').value = q;
      }
      executeQuery();
    }

    function copyPlanText() {
      const text = document.getElementById('plan-ascii-output').textContent;
      navigator.clipboard.writeText(text).then(() => {
        const btn = document.querySelector('#plan-viewport button');
        if (btn) {
          const orig = btn.textContent;
          btn.textContent = '✅ Copied!';
          setTimeout(() => { btn.textContent = orig; }, 1500);
        }
      });
    }

    function renderPlan(data) {
      const planText = data.plan || (data.rows && data.rows.length > 0 && data.rows[0].length > 0 ? String(data.rows[0][0]) : 'No physical plan returned.');
      document.getElementById('plan-ascii-output').textContent = planText;

      const dagContainer = document.getElementById('plan-dag-container');
      dagContainer.innerHTML = '';

      const lines = planText.split('\n').filter(l => l.trim().length > 0);
      if (lines.length === 0) return;

      const opColors = {
        'PROJECTION': '#58a6ff',
        'FILTER': '#f0883e',
        'INDEXSCAN': '#3fb950',
        'SCAN': '#3fb950',
        'SORT': '#bc8cff',
        'AGGREGATE': '#39c5bb',
        'SKIP': '#d29922',
        'LIMIT': '#d29922',
        'DISTINCT': '#f778ba',
        'MUTATE': '#f85149',
        'MERGEVERTEX': '#e3b341'
      };

      lines.forEach((line, idx) => {
        const indent = line.search(/\S/);
        const trimmed = line.trim();
        const firstWord = (trimmed.match(/^[A-Za-z0-9_]+/) || ['Operator'])[0].toUpperCase();
        const badgeColor = opColors[firstWord] || '#79c0ff';

        let details = '';
        const bracketMatch = trimmed.match(/\[(.*)\]/);
        if (bracketMatch) {
          details = bracketMatch[1];
        } else {
          details = trimmed.substring(firstWord.length).trim();
        }

        const stepDiv = document.createElement('div');
        stepDiv.style.cssText = `display:flex; flex-direction:column; align-items:center; margin-left:${Math.min(indent * 8, 80)}px;`;

        if (idx > 0) {
          const arrow = document.createElement('div');
          arrow.innerHTML = '↓';
          arrow.style.cssText = 'color:var(--text-muted); font-size:14px; margin:2px 0;';
          stepDiv.appendChild(arrow);
        }

        const card = document.createElement('div');
        card.style.cssText = `
          width:100%; max-width:680px; background:var(--bg-card);
          border:1px solid var(--border); border-left:4px solid ${badgeColor};
          border-radius:6px; padding:10px 14px; display:flex;
          justify-content:space-between; align-items:center;
          box-shadow:0 1px 3px rgba(0,0,0,0.2);
        `;

        card.innerHTML = `
          <div style="display:flex; flex-direction:column; gap:4px;">
            <div style="display:flex; align-items:center; gap:8px;">
              <span style="font-size:12px; font-weight:700; color:var(--text-bright); text-transform:uppercase;">${firstWord}</span>
              <span class="dash-badge" style="background:${badgeColor}22; color:${badgeColor}; border:1px solid ${badgeColor}44; font-size:10px;">Operator #${idx + 1}</span>
            </div>
            ${details ? `<div style="font-family:monospace; font-size:11px; color:var(--text-muted); word-break:break-all;">${details}</div>` : ''}
          </div>
          <div style="text-align:right;">
            <span style="font-size:10px; color:var(--text-muted);">Depth: ${Math.floor(indent / 2)}</span>
          </div>
        `;

        stepDiv.appendChild(card);
        dagContainer.appendChild(stepDiv);
      });
    }

    // Table Rendering
    function renderTable(data) {
      const thead = document.getElementById('table-header');
      const tbody = document.getElementById('table-body');
      thead.innerHTML = '';
      tbody.innerHTML = '';

      if (!data.columns || data.columns.length === 0) {
        thead.innerHTML = '<th>Result</th>';
        tbody.innerHTML = `<tr><td>${data.message || (data.error ? `<span style="color:var(--danger)">${data.error}</span>` : 'Success')}</td></tr>`;
        return;
      }

      for (const col of data.columns) {
        const th = document.createElement('th');
        th.textContent = col;
        thead.appendChild(th);
      }

      if (data.rows) {
        for (const row of data.rows) {
          const tr = document.createElement('tr');
          for (const cell of row) {
            const td = document.createElement('td');
            td.textContent = (typeof cell === 'object') ? JSON.stringify(cell) : cell;
            tr.appendChild(td);
          }
          tbody.appendChild(tr);
        }
      }
    }

    // Build Graph from Node/Edge Lists
    function buildGraph(nodes, edges) {
      const MAX_GRAPH_NODES = 250;
      let capped = false;
      const nodeMap = new Map();
      graphNodes = [];
      graphEdges = [];

      for (const n of nodes) {
        if (graphNodes.length >= MAX_GRAPH_NODES) {
          capped = true;
          break;
        }
        const node = {
          id: n.id,
          label: n.label || String(n.id),
          properties: n.properties || {},
          x: (Math.random() - 0.5) * 300,
          y: (Math.random() - 0.5) * 300,
          vx: 0,
          vy: 0
        };
        nodeMap.set(String(n.id), node);
        graphNodes.push(node);
      }

      for (const e of edges) {
        const src = nodeMap.get(String(e.source));
        const dst = nodeMap.get(String(e.target));
        if (src && dst) {
          graphEdges.push({
            source: src,
            target: dst,
            label: e.label || '',
            properties: e.properties || {}
          });
        }
      }

      if (capped) {
        document.getElementById('stat-graph').textContent = `${graphNodes.length} nodes (capped at 250 for 60 FPS), ${graphEdges.length} edges | Full data in Table view`;
      } else {
        document.getElementById('stat-graph').textContent = `${graphNodes.length} nodes, ${graphEdges.length} edges`;
      }
    }

    // Auto-extract graph from generic rows (e.g. [a.name, b.name] or [vertex_id, score])
    function extractGraphFromRows(data) {
      if (!data.columns || !data.rows || data.rows.length === 0) return;

      const MAX_GRAPH_NODES = 250;
      let capped = false;
      const nodeMap = new Map();
      graphNodes = [];
      graphEdges = [];

      // Case 1: 2-column or 3-column path traversal (source -> target)
      if (data.columns.length >= 2 && !data.columns[1].includes('score') && !data.columns[1].includes('distance')) {
        for (const row of data.rows) {
          if (graphNodes.length >= MAX_GRAPH_NODES) {
            capped = true;
            break;
          }
          const uId = String(row[0]);
          const vId = String(row[1]);

          if (!nodeMap.has(uId)) {
            const u = { id: uId, label: uId, properties: {}, x: (Math.random() - 0.5) * 200, y: (Math.random() - 0.5) * 200, vx: 0, vy: 0 };
            nodeMap.set(uId, u);
            graphNodes.push(u);
          }
          if (!nodeMap.has(vId)) {
            const v = { id: vId, label: vId, properties: {}, x: (Math.random() - 0.5) * 200, y: (Math.random() - 0.5) * 200, vx: 0, vy: 0 };
            nodeMap.set(vId, v);
            graphNodes.push(v);
          }

          graphEdges.push({
            source: nodeMap.get(uId),
            target: nodeMap.get(vId),
            label: data.columns.length > 2 ? String(row[2]) : ''
          });
        }
      }
      // Case 2: Analytical single vertex score table (e.g. vertex_id, score/community)
      else if (data.columns.includes('vertex_id')) {
        const idIdx = data.columns.indexOf('vertex_id');
        const metricIdx = idIdx === 0 ? 1 : 0;
        const metricName = data.columns[metricIdx];

        for (const row of data.rows) {
          if (graphNodes.length >= MAX_GRAPH_NODES) {
            capped = true;
            break;
          }
          const vid = String(row[idIdx]);
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
          graphNodes.push(node);
        }
      }

      if (capped) {
        document.getElementById('stat-graph').textContent = `${graphNodes.length} nodes (capped at 250 for 60 FPS), ${graphEdges.length} edges | Full data in Table view`;
      } else {
        document.getElementById('stat-graph').textContent = `${graphNodes.length} nodes, ${graphEdges.length} edges`;
      }
    }

    // Tabs & Navigation
    function switchView(viewName, el) {
      currentView = viewName;
      document.querySelectorAll('.nav-tab').forEach(t => t.classList.remove('active'));
      if (el) el.classList.add('active');
      document.querySelectorAll('.viewport').forEach(v => {
        v.classList.remove('active');
        v.style.display = 'none';
      });
      const vp = document.getElementById(`${viewName}-viewport`);
      if (vp) {
        vp.classList.add('active');
        vp.style.display = 'block';
      }
      if (viewName === 'graph') {
        physicsRunning = true;
        resizeCanvas();
      } else {
        physicsRunning = false;
      }
      if (viewName === 'unity') {
        renderUnityGraph();
      } else if (viewName === 'schema') {
        fetchSchema();
      } else if (viewName === 'cluster') {
        fetchClusterStatus();
      } else if (viewName === 'resources') {
        fetchResources();
      }
    }

    function selectNode(nodeId) {
      const node = graphNodes.find(n => String(n.id) === String(nodeId)) || unityNodes.find(n => String(n.id) === String(nodeId));
      if (node) {
        selectedEntity = node;
        openInspector(node);
      }
    }

    window.onUnityNodeSelected = function(nodeId) {
      console.log("[Unity Bridge] Node selected:", nodeId);
      selectNode(nodeId);
    };

    // 3D Spatial Visualizer Engine
    const unityCanvas = document.getElementById('unity-canvas');
    const unityCtx = unityCanvas ? unityCanvas.getContext('2d') : null;
    let unityNodes = [];
    let unityEdges = [];
    let unity3DRotX = 0.35;
    let unity3DRotY = 0.45;
    let unity3DZoom = 550;
    let unity3DPanX = 0;
    let unity3DPanY = 0;
    let unity3DDragging = false;
    let unity3DDragStart = { x: 0, y: 0 };
    let unity3DButton = 0;
    let unityPhysicsRunning = true;
    let unityAutoRotate = false;
    let unityHoveredNode = null;

    function resizeUnityCanvas() {
      if (!unityCanvas) return;
      const rect = unityCanvas.parentElement.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return;
      unityCanvas.width = rect.width * window.devicePixelRatio;
      unityCanvas.height = rect.height * window.devicePixelRatio;
      if (unityCtx) {
        unityCtx.setTransform(1, 0, 0, 1, 0, 0);
        unityCtx.scale(window.devicePixelRatio, window.devicePixelRatio);
      }
    }
    window.addEventListener('resize', resizeUnityCanvas);
    setTimeout(resizeUnityCanvas, 60);

    function sync3DGraph() {
      const nodeMap = new Map();
      unityNodes = graphNodes.map(n => {
        const u = {
          id: n.id,
          label: n.label,
          properties: n.properties || {},
          x: (Math.random() - 0.5) * 260,
          y: (Math.random() - 0.5) * 260,
          z: (Math.random() - 0.5) * 260,
          vx: 0,
          vy: 0,
          vz: 0,
          radius: 14
        };
        nodeMap.set(String(n.id), u);
        return u;
      });

      unityEdges = [];
      for (const e of graphEdges) {
        const src = nodeMap.get(String(e.source.id));
        const dst = nodeMap.get(String(e.target.id));
        if (src && dst) {
          unityEdges.push({
            source: src,
            target: dst,
            label: e.label || '',
            properties: e.properties || {}
          });
        }
      }
    }

    function stepUnity3DSimulation() {
      if (!unityPhysicsRunning || unityNodes.length === 0 || currentView !== 'unity') return;

      const repulsion = 5500;
      const springLength = 100;
      const springStrength = 0.04;
      const centerGravity = 0.015;

      // 3D Repulsion between nodes
      for (let i = 0; i < unityNodes.length; i++) {
        for (let j = i + 1; j < unityNodes.length; j++) {
          const a = unityNodes[i];
          const b = unityNodes[j];
          let dx = b.x - a.x;
          let dy = b.y - a.y;
          let dz = b.z - a.z;
          let dist = Math.sqrt(dx * dx + dy * dy + dz * dz) || 1;
          if (dist > 350) continue;
          let force = repulsion / (dist * dist);
          let fx = (dx / dist) * force;
          let fy = (dy / dist) * force;
          let fz = (dz / dist) * force;
          a.vx -= fx;
          a.vy -= fy;
          a.vz -= fz;
          b.vx += fx;
          b.vy += fy;
          b.vz += fz;
        }
      }

      // 3D Spring Attraction along edges
      for (const e of unityEdges) {
        const a = e.source;
        const b = e.target;
        if (!a || !b) continue;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dz = b.z - a.z;
        let dist = Math.sqrt(dx * dx + dy * dy + dz * dz) || 1;
        let force = (dist - springLength) * springStrength;
        let fx = (dx / dist) * force;
        let fy = (dy / dist) * force;
        let fz = (dz / dist) * force;
        a.vx += fx;
        a.vy += fy;
        a.vz += fz;
        b.vx -= fx;
        b.vy -= fy;
        b.vz -= fz;
      }

      // Damping & Center Gravity
      for (const n of unityNodes) {
        n.vx -= n.x * centerGravity;
        n.vy -= n.y * centerGravity;
        n.vz -= n.z * centerGravity;
        n.vx *= 0.88;
        n.vy *= 0.88;
        n.vz *= 0.88;
        n.x += n.vx;
        n.y += n.vy;
        n.z += n.vz;
      }
    }

    // 3D Perspective Projection
    function project3D(x, y, z, cx, cy) {
      // Rotation Y
      const cosY = Math.cos(unity3DRotY);
      const sinY = Math.sin(unity3DRotY);
      const x1 = x * cosY + z * sinY;
      const z1 = -x * sinY + z * cosY;

      // Rotation X
      const cosX = Math.cos(unity3DRotX);
      const sinX = Math.sin(unity3DRotX);
      const y2 = y * cosX - z1 * sinX;
      const z2 = y * sinX + z1 * cosX;

      const camDist = unity3DZoom;
      const zCam = z2 + camDist;
      if (zCam <= 20) return null;

      const fov = 500;
      const scale = fov / zCam;
      const sx = cx + unity3DPanX + x1 * scale;
      const sy = cy + unity3DPanY + y2 * scale;

      return { sx, sy, scale, depth: z2, zCam };
    }

    function renderUnity3DLoop() {
      if (currentView === 'unity' && unityCtx && unityCanvas) {
        const w = unityCanvas.clientWidth;
        const h = unityCanvas.clientHeight;
        const cx = w / 2;
        const cy = h / 2;

        if (unityAutoRotate) {
          unity3DRotY += 0.005;
        }

        stepUnity3DSimulation();

        unityCtx.clearRect(0, 0, w, h);

        // Draw 3D Grid floor
        unityCtx.save();
        unityCtx.strokeStyle = 'rgba(48, 54, 61, 0.25)';
        unityCtx.lineWidth = 1;
        const gridSize = 200;
        const gridStep = 40;
        const floorY = 120;
        for (let gx = -gridSize; gx <= gridSize; gx += gridStep) {
          const p1 = project3D(gx, floorY, -gridSize, cx, cy);
          const p2 = project3D(gx, floorY, gridSize, cx, cy);
          if (p1 && p2) {
            unityCtx.beginPath();
            unityCtx.moveTo(p1.sx, p1.sy);
            unityCtx.lineTo(p2.sx, p2.sy);
            unityCtx.stroke();
          }
        }
        for (let gz = -gridSize; gz <= gridSize; gz += gridStep) {
          const p1 = project3D(-gridSize, floorY, gz, cx, cy);
          const p2 = project3D(gridSize, floorY, gz, cx, cy);
          if (p1 && p2) {
            unityCtx.beginPath();
            unityCtx.moveTo(p1.sx, p1.sy);
            unityCtx.lineTo(p2.sx, p2.sy);
            unityCtx.stroke();
          }
        }
        unityCtx.restore();

        // Project nodes
        const projectedNodes = [];
        for (const n of unityNodes) {
          const p = project3D(n.x, n.y, n.z, cx, cy);
          if (p) {
            projectedNodes.push({
              node: n,
              sx: p.sx,
              sy: p.sy,
              scale: p.scale,
              depth: p.depth,
              radius: Math.max(4, n.radius * p.scale)
            });
          }
        }

        // Draw Edges
        unityCtx.save();
        for (const e of unityEdges) {
          const p1 = project3D(e.source.x, e.source.y, e.source.z, cx, cy);
          const p2 = project3D(e.target.x, e.target.y, e.target.z, cx, cy);
          if (p1 && p2) {
            const avgDepth = (p1.depth + p2.depth) / 2;
            const alpha = Math.max(0.15, Math.min(0.7, 0.4 - avgDepth / 800));
            unityCtx.strokeStyle = `rgba(88, 166, 255, ${alpha})`;
            unityCtx.lineWidth = Math.max(1, 2 * ((p1.scale + p2.scale) / 2));
            unityCtx.beginPath();
            unityCtx.moveTo(p1.sx, p1.sy);
            unityCtx.lineTo(p2.sx, p2.sy);
            unityCtx.stroke();

            if (e.label && (p1.scale + p2.scale) / 2 > 0.8) {
              const mx = (p1.sx + p2.sx) / 2;
              const my = (p1.sy + p2.sy) / 2;
              unityCtx.fillStyle = 'rgba(139, 148, 158, 0.8)';
              unityCtx.font = '9px monospace';
              unityCtx.fillText(e.label, mx + 4, my - 4);
            }
          }
        }
        unityCtx.restore();

        // Depth sort nodes (farthest first)
        projectedNodes.sort((a, b) => a.depth - b.depth);

        // Draw Shaded 3D Spheres
        for (const pn of projectedNodes) {
          const n = pn.node;
          const r = pn.radius;
          const isHovered = (n === unityHoveredNode);
          const isSelected = (selectedEntity && String(selectedEntity.id) === String(n.id));
          const baseColor = getNodeColor(n.label, n.id);

          // Selection / Hover Halo
          if (isSelected || isHovered) {
            unityCtx.beginPath();
            unityCtx.arc(pn.sx, pn.sy, r * 1.5, 0, Math.PI * 2);
            unityCtx.fillStyle = isSelected ? 'rgba(88, 166, 255, 0.35)' : 'rgba(255, 255, 255, 0.2)';
            unityCtx.fill();
          }

          // Shaded Sphere (Radial Gradient)
          const grad = unityCtx.createRadialGradient(
            pn.sx - r * 0.35, pn.sy - r * 0.35, r * 0.05,
            pn.sx, pn.sy, r
          );
          grad.addColorStop(0, '#ffffff');
          grad.addColorStop(0.35, baseColor);
          grad.addColorStop(1, '#05070a');

          unityCtx.beginPath();
          unityCtx.arc(pn.sx, pn.sy, r, 0, Math.PI * 2);
          unityCtx.fillStyle = grad;
          unityCtx.fill();

          if (isSelected) {
            unityCtx.strokeStyle = '#ffffff';
            unityCtx.lineWidth = 2;
            unityCtx.stroke();
          }

          // 3D Billboard Label
          if (r > 6) {
            unityCtx.fillStyle = '#f0f6fc';
            unityCtx.font = `${Math.max(9, Math.round(11 * pn.scale))}px sans-serif`;
            unityCtx.textAlign = 'center';
            unityCtx.textBaseline = 'middle';
            const displayLabel = String(n.label || n.id).substring(0, 5);
            unityCtx.fillText(displayLabel, pn.sx, pn.sy);

            if (r > 10) {
              unityCtx.fillStyle = 'rgba(139, 148, 158, 0.9)';
              unityCtx.font = '9px monospace';
              unityCtx.fillText(String(n.id), pn.sx, pn.sy + r + 12);
            }
          }
        }
      }

      requestAnimationFrame(renderUnity3DLoop);
    }
    requestAnimationFrame(renderUnity3DLoop);

    // 3D Controls & Event Handlers
    if (unityCanvas) {
      unityCanvas.addEventListener('mousedown', (e) => {
        unity3DDragging = true;
        unity3DButton = e.button;
        unity3DDragStart = { x: e.clientX, y: e.clientY };

        // Raycast click
        const rect = unityCanvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const w = unityCanvas.clientWidth;
        const h = unityCanvas.clientHeight;
        const cx = w / 2;
        const cy = h / 2;

        let bestNode = null;
        let bestDist = Infinity;
        for (const n of unityNodes) {
          const p = project3D(n.x, n.y, n.z, cx, cy);
          if (p) {
            const r = Math.max(6, n.radius * p.scale);
            const dx = mx - p.sx;
            const dy = my - p.sy;
            const dist = Math.sqrt(dx * dx + dy * dy);
            if (dist <= r + 4 && dist < bestDist) {
              bestDist = dist;
              bestNode = n;
            }
          }
        }

        if (bestNode) {
          unityHoveredNode = bestNode;
          selectedEntity = bestNode;
          openInspector(bestNode);
          if (typeof window.onUnityNodeSelected === 'function') {
            window.onUnityNodeSelected(bestNode.id);
          }
        }
      });

      window.addEventListener('mousemove', (e) => {
        if (!unity3DDragging || currentView !== 'unity') {
          if (currentView === 'unity' && unityCanvas) {
            const rect = unityCanvas.getBoundingClientRect();
            const mx = e.clientX - rect.left;
            const my = e.clientY - rect.top;
            const cx = unityCanvas.clientWidth / 2;
            const cy = unityCanvas.clientHeight / 2;
            unityHoveredNode = null;
            for (const n of unityNodes) {
              const p = project3D(n.x, n.y, n.z, cx, cy);
              if (p) {
                const r = Math.max(6, n.radius * p.scale);
                const dx = mx - p.sx;
                const dy = my - p.sy;
                if (Math.sqrt(dx * dx + dy * dy) <= r + 2) {
                  unityHoveredNode = n;
                  unityCanvas.style.cursor = 'pointer';
                  break;
                }
              }
            }
            if (!unityHoveredNode) unityCanvas.style.cursor = 'grab';
          }
          return;
        }

        const dx = e.clientX - unity3DDragStart.x;
        const dy = e.clientY - unity3DDragStart.y;
        unity3DDragStart = { x: e.clientX, y: e.clientY };

        if (unity3DButton === 2 || e.shiftKey) {
          // Pan
          unity3DPanX += dx;
          unity3DPanY += dy;
        } else {
          // Orbit
          unity3DRotY += dx * 0.008;
          unity3DRotX += dy * 0.008;
          unity3DRotX = Math.max(-Math.PI / 2.1, Math.min(Math.PI / 2.1, unity3DRotX));
        }
      });

      window.addEventListener('mouseup', () => {
        unity3DDragging = false;
      });

      unityCanvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        const factor = e.deltaY > 0 ? 1.08 : 0.92;
        unity3DZoom = Math.max(100, Math.min(2500, unity3DZoom * factor));
      }, { passive: false });

      unityCanvas.addEventListener('contextmenu', (e) => e.preventDefault());
    }

    function zoomUnity3D(factor) {
      unity3DZoom = Math.max(100, Math.min(2500, unity3DZoom / factor));
    }

    function resetUnity3DView() {
      unity3DRotX = 0.35;
      unity3DRotY = 0.45;
      unity3DZoom = 550;
      unity3DPanX = 0;
      unity3DPanY = 0;
    }

    function toggleUnityPhysics() {
      unityPhysicsRunning = !unityPhysicsRunning;
      const btn = document.getElementById('unity-physics-btn');
      if (btn) btn.textContent = unityPhysicsRunning ? '⏸' : '▶';
    }

    function toggleUnityAutoRotate() {
      unityAutoRotate = !unityAutoRotate;
      const btn = document.getElementById('unity-rotate-btn');
      if (btn) btn.style.color = unityAutoRotate ? 'var(--accent)' : 'inherit';
    }

    function renderUnityGraph() {
      resizeUnityCanvas();
      sync3DGraph();

      const badge = document.getElementById('unity-status-badge');
      const statsTitle = document.getElementById('unity-stats-title');
      const count = unityNodes.length;

      if (statsTitle) {
        statsTitle.textContent = `3D Topology: ${count} Nodes, ${unityEdges.length} Edges`;
      }

      if (window.unityInstance) {
        const payload = {
          nodes: unityNodes.map(n => ({ id: n.id, label: n.label, properties: n.properties })),
          edges: unityEdges.map(e => ({ src: e.source.id, dst: e.target.id, label: e.label }))
        };
        window.unityInstance.SendMessage('GraphController', 'ReceiveGraphData', JSON.stringify(payload));
        if (badge) {
          badge.textContent = `Unity WebGL Active | Drag to Orbit | Click Sphere to Inspect`;
          badge.style.color = '#3fb950';
        }
        return;
      }

      if (badge) {
        if (count > 0) {
          badge.textContent = `${count} 3D Spheres Active | Drag to Orbit | Scroll to Zoom | Click to Inspect`;
          badge.style.color = '#3fb950';
        } else {
          badge.textContent = `Ready: Execute a query to visualize 3D graph nodes`;
          badge.style.color = '#58a6ff';
        }
      }
    }

    // Modal Helpers
    function openModal(id) {
      const el = document.getElementById(id);
      if (el) el.style.display = 'flex';
    }
    function closeModal(id) {
      const el = document.getElementById(id);
      if (el) el.style.display = 'none';
    }

    // Modal Triggers
    function openCreateVertexModal() {
      document.getElementById('new-vertex-name').value = '';
      document.getElementById('new-vertex-pk').value = 'id';
      document.getElementById('new-vertex-props').value = 'name: STRING, created_at: INT64';
      openModal('modal-create-vertex');
      document.getElementById('new-vertex-name').focus();
    }

    function openCreateEdgeModal() {
      document.getElementById('new-edge-name').value = '';
      document.getElementById('new-edge-props').value = 'weight: FLOAT64, created_at: INT64';
      openModal('modal-create-edge');
      document.getElementById('new-edge-name').focus();
    }

    function openAddPropertyModal(targetKind, targetName) {
      document.getElementById('add-prop-target-kind').value = targetKind;
      document.getElementById('add-prop-target-name').value = targetName;
      document.getElementById('add-prop-target-display').value = `${targetKind.toUpperCase()}: ${targetName}`;
      document.getElementById('add-prop-name').value = '';
      document.getElementById('modal-add-prop-title').textContent = `➕ Add Property to ${targetKind === 'vertex' ? 'Tag' : 'Edge'} [${targetName}]`;
      openModal('modal-add-prop');
      document.getElementById('add-prop-name').focus();
    }

    // DDL Execution Helper
    async function executeQueryDirect(query) {
      const endpoint = document.getElementById('cluster-url').value.trim();
      try {
        const resp = await fetch('/api/query', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ query: query, endpoint: endpoint })
        });
        const data = await resp.json();
        if (data.status !== 'ok') {
          alert('DDL Error: ' + (data.error || JSON.stringify(data)));
          return false;
        }
        addHistory(query, true);
        return true;
      } catch (err) {
        alert('Network Error: ' + err.message);
        return false;
      }
    }

    // Modal Submissions
    async function submitCreateVertex() {
      const name = document.getElementById('new-vertex-name').value.trim();
      const propsStr = document.getElementById('new-vertex-props').value.trim();
      if (!name) {
        alert('Please enter a vertex tag name.');
        return;
      }
      let ddl = `CREATE VERTEX ${name}`;
      if (propsStr) {
        ddl += ` (${propsStr})`;
      } else {
        ddl += ` (id: INT64)`;
      }
      ddl += `;`;

      closeModal('modal-create-vertex');
      const ok = await executeQueryDirect(ddl);
      if (ok) {
        await fetchSchema();
      }
    }

    async function submitCreateEdge() {
      const name = document.getElementById('new-edge-name').value.trim();
      const propsStr = document.getElementById('new-edge-props').value.trim();
      if (!name) {
        alert('Please enter an edge type name.');
        return;
      }
      let ddl = `CREATE EDGE ${name}`;
      if (propsStr) {
        ddl += ` (${propsStr})`;
      }
      ddl += `;`;

      closeModal('modal-create-edge');
      const ok = await executeQueryDirect(ddl);
      if (ok) {
        await fetchSchema();
      }
    }

    async function submitAddProperty() {
      const kind = document.getElementById('add-prop-target-kind').value;
      const targetName = document.getElementById('add-prop-target-name').value;
      const propName = document.getElementById('add-prop-name').value.trim();
      const propType = document.getElementById('add-prop-type').value;

      if (!propName) {
        alert('Please enter a property name.');
        return;
      }

      let ddl = '';
      if (kind === 'vertex') {
        ddl = `ALTER VERTEX ${targetName} ADD ${propName}: ${propType};`;
      } else {
        ddl = `ALTER EDGE ${targetName} ADD ${propName}: ${propType};`;
      }

      closeModal('modal-add-prop');
      const ok = await executeQueryDirect(ddl);
      if (ok) {
        await fetchSchema();
      }
    }

    async function dropVertexLabel(labelName) {
      if (!confirm(`Are you sure you want to drop vertex tag [${labelName}]?\nAll associated schema metadata and properties will be removed.`)) {
        return;
      }
      const ok = await executeQueryDirect(`DROP VERTEX ${labelName};`);
      if (ok) await fetchSchema();
    }

    async function dropEdgeType(typeName) {
      if (!confirm(`Are you sure you want to drop edge type [${typeName}]?\nAll associated schema metadata will be removed.`)) {
        return;
      }
      const ok = await executeQueryDirect(`DROP EDGE ${typeName};`);
      if (ok) await fetchSchema();
    }

    async function dropVertexProperty(labelName, propName) {
      if (!confirm(`Drop property "${propName}" from tag [${labelName}]?`)) {
        return;
      }
      const ok = await executeQueryDirect(`ALTER VERTEX ${labelName} DROP ${propName};`);
      if (ok) await fetchSchema();
    }

    async function createIndex(labelName, propName) {
      const ok = await executeQueryDirect(`CREATE INDEX ON :${labelName}(${propName});`);
      if (ok) await fetchSchema();
    }

    async function dropIndex(labelName, propName) {
      const ok = await executeQueryDirect(`DROP INDEX ON :${labelName}(${propName});`);
      if (ok) await fetchSchema();
    }

    function queryTag(labelName) {
      setQuery(`MATCH (n:${labelName}) RETURN n LIMIT 50;`);
      executeQuery();
      switchView('graph', document.getElementById('tab-btn-graph'));
    }

    function queryEdge(edgeType) {
      setQuery(`MATCH ()-[r:${edgeType}]->() RETURN r LIMIT 50;`);
      executeQuery();
      switchView('graph', document.getElementById('tab-btn-graph'));
    }

    // Fetch Schema and Render UI
    async function fetchSchema() {
      const endpoint = document.getElementById('cluster-url').value.trim();
      try {
        const resp = await fetch(`/api/schema?endpoint=${encodeURIComponent(endpoint)}`);
        if (resp.ok) {
          const schema = await resp.json();
          renderSchemaUI(schema);
        }
      } catch (err) {
        console.error("Failed to fetch schema:", err);
      }
    }

    function renderSchemaUI(schema) {
      const vSchemas = schema.vertices || schema.vertex_schemas || [];
      const eSchemas = schema.edges || schema.edge_schemas || [];

      // Update Badges
      const vBadge = document.getElementById('schema-total-tags-badge');
      if (vBadge) vBadge.textContent = `${vSchemas.length} Vertex Tags`;
      const eBadge = document.getElementById('schema-total-edges-badge');
      if (eBadge) eBadge.textContent = `${eSchemas.length} Edge Types`;

      const vCountLabel = document.getElementById('schema-v-count-label');
      if (vCountLabel) vCountLabel.textContent = `${vSchemas.length} defined tags`;
      const eCountLabel = document.getElementById('schema-e-count-label');
      if (eCountLabel) eCountLabel.textContent = `${eSchemas.length} defined types`;

      // Update Sidebar Schema Tab
      const sbV = document.getElementById('schema-vertices');
      if (sbV) {
        if (vSchemas.length === 0) {
          sbV.innerHTML = '<div style="font-size:11px; color:var(--text-muted); font-style:italic;">No vertex tags defined.</div>';
        } else {
          sbV.innerHTML = '';
          vSchemas.forEach(v => {
            const labelName = v.label || v.name;
            const propsCount = (v.properties || []).length;
            const span = document.createElement('span');
            span.className = 'schema-tag';
            span.textContent = `${labelName} (${propsCount}p)`;
            span.onclick = () => queryTag(labelName);
            sbV.appendChild(span);
          });
        }
      }

      const sbE = document.getElementById('schema-edges');
      if (sbE) {
        if (eSchemas.length === 0) {
          sbE.innerHTML = '<div style="font-size:11px; color:var(--text-muted); font-style:italic;">No edge types defined.</div>';
        } else {
          sbE.innerHTML = '';
          eSchemas.forEach(e => {
            const edgeName = e.edge_type_name || e.name;
            const span = document.createElement('span');
            span.className = 'schema-tag edge';
            span.textContent = edgeName;
            span.onclick = () => queryEdge(edgeName);
            sbE.appendChild(span);
          });
        }
      }

      // Render Vertex Cards Grid
      const vGrid = document.getElementById('schema-vertex-grid');
      if (vGrid) {
        if (vSchemas.length === 0) {
          vGrid.innerHTML = `
            <div style="grid-column: 1/-1; padding:30px; text-align:center; color:var(--text-muted); background:var(--bg-card); border-radius:8px; border:1px dashed var(--border);">
              <div style="font-size:24px; margin-bottom:8px;">🏷️</div>
              <div style="font-weight:600; color:var(--text); margin-bottom:4px;">No Vertex Tags Defined</div>
              <div style="font-size:12px; margin-bottom:12px;">Create a new vertex tag schema or run <code>CREATE VERTEX TagName (id: INT64, ...);</code></div>
              <button class="btn btn-primary btn-sm" onclick="openCreateVertexModal()">➕ Create First Tag</button>
            </div>
          `;
        } else {
          vGrid.innerHTML = '';
          vSchemas.forEach(v => {
            const card = document.createElement('div');
            card.className = 'schema-card';

            const labelName = v.label || v.name;
            const labelId = v.label_id !== undefined ? v.label_id : '-';
            const indexes = new Set(v.indexes || []);
            const pk = v.primary_key || '';
            const props = v.properties || [];

            let propRows = '';
            props.forEach(p => {
              const pType = p.type || p.data_type || 'String';
              const hasIdx = indexes.has(p.name) || p.indexed === true;
              const isPk = (p.name === pk);
              propRows += `
                <tr>
                  <td style="font-weight:600; font-family:monospace; color:var(--text-bright);">${p.name}</td>
                  <td><span class="prop-type-badge">${pType}</span></td>
                  <td>
                    ${isPk ? '<span class="index-badge" style="background:rgba(210,153,34,0.15); color:var(--yellow); border-color:rgba(210,153,34,0.3);">🔑 PK</span> ' : ''}
                    ${hasIdx ? '<span class="index-badge">⚡ INDEX</span>' : '<span style="color:var(--text-muted);">-</span>'}
                  </td>
                  <td style="text-align:right;">
                    <div style="display:inline-flex; gap:4px;">
                      ${hasIdx 
                        ? `<button class="btn btn-secondary btn-sm" style="font-size:10px; padding:1px 6px;" title="Drop Index" onclick="dropIndex('${labelName}', '${p.name}')">Drop Idx</button>`
                        : `<button class="btn btn-secondary btn-sm" style="font-size:10px; padding:1px 6px;" title="Create Secondary Index" onclick="createIndex('${labelName}', '${p.name}')">+ Index</button>`
                      }
                      <button class="btn btn-danger btn-sm" style="font-size:10px; padding:1px 6px;" title="Drop Property" onclick="dropVertexProperty('${labelName}', '${p.name}')">🗑</button>
                    </div>
                  </td>
                </tr>
              `;
            });

            card.innerHTML = `
              <div class="schema-card-header">
                <div style="display:flex; align-items:center; gap:8px;">
                  <span style="font-size:16px;">🏷️</span>
                  <div>
                    <span style="font-size:14px; font-weight:700; color:var(--accent); font-family:monospace;">${labelName}</span>
                    <span style="font-size:10px; color:var(--text-muted); margin-left:6px;">ID: ${labelId}</span>
                  </div>
                </div>
                <div style="display:flex; gap:4px;">
                  <button class="btn btn-secondary btn-sm" style="font-size:11px; padding:2px 8px;" onclick="queryTag('${labelName}')" title="Query Vertices">🔍 Query</button>
                  <button class="btn btn-primary btn-sm" style="font-size:11px; padding:2px 8px;" onclick="openAddPropertyModal('vertex', '${labelName}')" title="Add Property">➕ Prop</button>
                  <button class="btn btn-danger btn-sm" style="font-size:11px; padding:2px 8px;" onclick="dropVertexLabel('${labelName}')" title="Drop Tag">🗑</button>
                </div>
              </div>
              <table class="schema-prop-table">
                <thead>
                  <tr>
                    <th>Property</th>
                    <th>Type</th>
                    <th>Flags</th>
                    <th style="text-align:right;">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  ${propRows || '<tr><td colspan="4" style="text-align:center; color:var(--text-muted);">No properties defined</td></tr>'}
                </tbody>
              </table>
            `;
            vGrid.appendChild(card);
          });
        }
      }

      // Render Edge Cards Grid
      const eGrid = document.getElementById('schema-edge-grid');
      if (eGrid) {
        if (eSchemas.length === 0) {
          eGrid.innerHTML = `
            <div style="grid-column: 1/-1; padding:30px; text-align:center; color:var(--text-muted); background:var(--bg-card); border-radius:8px; border:1px dashed var(--border);">
              <div style="font-size:24px; margin-bottom:8px;">➡️</div>
              <div style="font-weight:600; color:var(--text); margin-bottom:4px;">No Edge Types Defined</div>
              <div style="font-size:12px; margin-bottom:12px;">Create a new edge schema or run <code>CREATE EDGE EdgeType (weight: FLOAT64, ...);</code></div>
              <button class="btn btn-primary btn-sm" style="background:#8957e5;" onclick="openCreateEdgeModal()">➕ Create First Edge Type</button>
            </div>
          `;
        } else {
          eGrid.innerHTML = '';
          eSchemas.forEach(e => {
            const card = document.createElement('div');
            card.className = 'schema-card';

            const edgeName = e.edge_type_name || e.name;
            const edgeId = e.edge_type !== undefined ? e.edge_type : (e.type_id !== undefined ? e.type_id : '-');
            const props = e.properties || [];

            let propRows = '';
            props.forEach(p => {
              const pType = p.type || p.data_type || 'String';
              propRows += `
                <tr>
                  <td style="font-weight:600; font-family:monospace; color:var(--text-bright);">${p.name}</td>
                  <td><span class="prop-type-badge">${pType}</span></td>
                  <td><span style="color:var(--text-muted);">-</span></td>
                </tr>
              `;
            });

            card.innerHTML = `
              <div class="schema-card-header">
                <div style="display:flex; align-items:center; gap:8px;">
                  <span style="font-size:16px;">➡️</span>
                  <div>
                    <span style="font-size:14px; font-weight:700; color:var(--purple); font-family:monospace;">${edgeName}</span>
                    <span style="font-size:10px; color:var(--text-muted); margin-left:6px;">ID: ${edgeId}</span>
                  </div>
                </div>
                <div style="display:flex; gap:4px;">
                  <button class="btn btn-secondary btn-sm" style="font-size:11px; padding:2px 8px;" onclick="queryEdge('${edgeName}')" title="Query Edges">🔍 Query</button>
                  <button class="btn btn-primary btn-sm" style="font-size:11px; padding:2px 8px;" onclick="openAddPropertyModal('edge', '${edgeName}')" title="Add Property">➕ Prop</button>
                  <button class="btn btn-danger btn-sm" style="font-size:11px; padding:2px 8px;" onclick="dropEdgeType('${edgeName}')" title="Drop Edge Type">🗑</button>
                </div>
              </div>
              <table class="schema-prop-table">
                <thead>
                  <tr>
                    <th>Property</th>
                    <th>Type</th>
                    <th>Flags</th>
                  </tr>
                </thead>
                <tbody>
                  ${propRows || '<tr><td colspan="3" style="text-align:center; color:var(--text-muted);">No properties defined</td></tr>'}
                </tbody>
              </table>
            `;
            eGrid.appendChild(card);
          });
        }
      }
    }

    function switchSidebarTab(tabName, el) {
      document.querySelectorAll('.sidebar-tab').forEach(t => t.classList.remove('active'));
      el.classList.add('active');
      document.getElementById('tab-templates').style.display = tabName === 'templates' ? 'block' : 'none';
      document.getElementById('tab-schema').style.display = tabName === 'schema' ? 'block' : 'none';
      document.getElementById('tab-cluster').style.display = tabName === 'cluster' ? 'block' : 'none';
      document.getElementById('tab-history').style.display = tabName === 'history' ? 'block' : 'none';
      if (tabName === 'cluster') fetchClusterStatus();
      if (tabName === 'schema') fetchSchema();
    }

    function setQuery(text) {
      document.getElementById('query-input').value = text;
      document.getElementById('query-input').focus();
    }

    function clearQuery() {
      document.getElementById('query-input').value = '';
    }

    function addHistory(query, success) {
      queryHistory.unshift({ query, time: new Date().toLocaleTimeString(), success });
      if (queryHistory.length > 30) queryHistory.pop();
      const list = document.getElementById('history-list');
      list.innerHTML = '';
      for (const item of queryHistory) {
        const div = document.createElement('div');
        div.className = 'history-item';
        div.onclick = () => setQuery(item.query);
        div.innerHTML = `<div class="template-title" style="color:${item.success ? 'var(--text-bright)' : 'var(--danger)'}">${item.time}</div><div class="template-query">${item.query}</div>`;
        list.appendChild(div);
      }
    }

    // Hotkey: Cmd+Enter / Ctrl+Enter to Run
    document.getElementById('query-input').addEventListener('keydown', (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        executeQuery();
      }
    });

    // Cluster Status Polling & Rendering
    async function fetchClusterStatus() {
      const endpoint = document.getElementById('cluster-url').value.trim();
      try {
        const resp = await fetch(`/api/cluster/status?endpoint=${encodeURIComponent(endpoint)}`);
        if (resp.ok) {
          const data = await resp.json();
          updateClusterUI(data);
        }
      } catch (err) {
        console.error("Failed to fetch cluster status:", err);
      }
    }

    function updateClusterUI(data) {
      const isConnected = data.status === 'connected';
      const pill = document.getElementById('cluster-status-pill');
      const text = document.getElementById('cluster-status-text');
      if (isConnected) {
        pill.className = 'status-indicator';
        text.textContent = `Connected (RF=${data.replication_factor || 3}, ${data.total_nodes || 3} Nodes)`;
      } else {
        pill.className = 'status-indicator offline';
        text.textContent = 'Disconnected';
      }

      // Update Header Node Pills
      const nodePillsContainer = document.getElementById('header-node-pills');
      if (nodePillsContainer && data.ring_nodes && data.ring_nodes.length > 0) {
        nodePillsContainer.innerHTML = '';
        data.ring_nodes.forEach(n => {
          const div = document.createElement('div');
          const isCurrent = data.node_id === n.node_id;
          div.className = `node-pill ${isCurrent ? 'active' : ''}`;
          div.title = `HTTP: ${n.http_url} | Flight: :${n.flight_port}`;
          div.textContent = `Peer #${n.node_id}`;
          div.onclick = () => {
            if (n.http_url) {
              document.getElementById('cluster-url').value = n.http_url;
              fetchClusterStatus();
              fetchResources();
            }
          };
          nodePillsContainer.appendChild(div);
        });
      }

      // Update Cluster Viewport Badges & Cards
      const rfBadge = document.getElementById('cluster-rf-badge');
      if (rfBadge) rfBadge.textContent = `RF = ${data.replication_factor || 3} (${(data.replication_mode || 'SYNC').toUpperCase()})`;
      const gpuBadge = document.getElementById('cluster-gpu-badge');
      if (gpuBadge) {
        if (data.gpu_enabled) {
          gpuBadge.className = 'dash-badge green';
          gpuBadge.textContent = `GPU Active (#${data.gpu_device || 0})`;
        } else {
          gpuBadge.className = 'dash-badge gray';
          gpuBadge.textContent = 'GPU Disabled';
        }
      }
      const rfVal = document.getElementById('cluster-rf-val');
      if (rfVal) rfVal.textContent = `RF = ${data.replication_factor || 3}`;
      const rfSub = document.getElementById('cluster-rf-sub');
      if (rfSub) rfSub.textContent = `Mode: ${data.replication_mode || 'SYNC'}`;
      const nodesCount = document.getElementById('cluster-nodes-count');
      if (nodesCount) nodesCount.textContent = `${data.total_nodes || 3} Peers`;
      const partitionsCount = document.getElementById('cluster-partitions-count');
      if (partitionsCount) partitionsCount.textContent = `${data.partitions || 8} Partitions per Node`;
      const gpuVal = document.getElementById('cluster-gpu-val');
      if (gpuVal) gpuVal.textContent = data.gpu_enabled ? `${data.gpu_backend || 'Metal'} (#${data.gpu_device || 0})` : 'Disabled';
      const gpuSub = document.getElementById('cluster-gpu-sub');
      if (gpuSub) gpuSub.textContent = data.gpu_enabled ? `Threshold: ${data.gpu_threshold || 10000} edges` : 'Pass --enable-gpu to activate';

      // Update Table of Nodes
      const tbody = document.getElementById('cluster-nodes-table-body');
      if (tbody && data.ring_nodes) {
        tbody.innerHTML = '';
        data.ring_nodes.forEach(n => {
          const tr = document.createElement('tr');
          const isCurrent = data.node_id === n.node_id;
          tr.innerHTML = `
            <td style="font-weight:600; color:var(--text-bright);">Peer Node #${n.node_id} ${isCurrent ? '<span style="font-size:10px; color:var(--accent);">(Current)</span>' : ''}</td>
            <td><span class="dash-badge blue">Peer</span></td>
            <td><code>${n.http_url}</code></td>
            <td><code>:${n.flight_port}</code></td>
            <td><code>:${n.client_flight_port || '-'}</code></td>
            <td><code>u % ${data.total_nodes || 3} == ${n.node_id - 1}</code></td>
            <td><span class="dash-badge green">UP</span></td>
            <td><button class="node-action-btn" onclick="document.getElementById('cluster-url').value='${n.http_url}'; fetchClusterStatus(); fetchResources();">Connect</button></td>
          `;
          tbody.appendChild(tr);
        });
      }

      // Update Sidebar Cluster Tab
      const sbNodes = document.getElementById('sidebar-cluster-nodes');
      if (sbNodes && data.ring_nodes) {
        sbNodes.innerHTML = '';
        data.ring_nodes.forEach(n => {
          const item = document.createElement('div');
          item.className = 'template-item';
          item.onclick = () => {
            document.getElementById('cluster-url').value = n.http_url;
            fetchClusterStatus();
            fetchResources();
          };
          item.innerHTML = `<div class="template-title">Node #${n.node_id} (:${n.flight_port})</div><div class="template-query">${n.http_url}</div>`;
          sbNodes.appendChild(item);
        });
      }

      // Update footer
      const footerCluster = document.getElementById('footer-cluster-info');
      if (footerCluster) footerCluster.textContent = `Cluster: Leaderless Ring (${data.total_nodes || 3} Nodes, RF=${data.replication_factor || 3})`;
      const footerGpu = document.getElementById('footer-gpu-info');
      if (footerGpu) footerGpu.textContent = `GPU: ${data.gpu_enabled ? `${data.gpu_backend || 'Metal'} (#${data.gpu_device || 0})` : 'Disabled'}`;
    }

    async function fetchResources() {
      const endpoint = document.getElementById('cluster-url').value.trim();
      try {
        const resp = await fetch(`/api/resources?endpoint=${encodeURIComponent(endpoint)}`);
        if (resp.ok) {
          const data = await resp.json();
          updateResourcesUI(data);
        }
      } catch (err) {
        console.error("Failed to fetch resources:", err);
      }
    }

    // Telemetry & Sparklines State
    const telemetryHistory = {
      qps: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      latency: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      ram: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    };
    let lastQueryCount = 0;
    let lastQueryPollTime = Date.now();

    function recordQueryLatency(latMs) {
      telemetryHistory.latency.push(latMs);
      if (telemetryHistory.latency.length > 20) telemetryHistory.latency.shift();
      const sparkLat = document.getElementById('spark-lat-val');
      if (sparkLat) sparkLat.textContent = `${latMs.toFixed(1)} ms`;
      renderSparkline('sparkline-latency', telemetryHistory.latency, '#3fb950', 'rgba(63, 185, 80, 0.15)');
    }

    function renderSparkline(svgId, points, strokeColor, fillColor) {
      const svg = document.getElementById(svgId);
      if (!svg) return;
      const width = svg.clientWidth || 240;
      const height = 48;
      svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
      svg.innerHTML = '';

      if (points.length < 2) return;
      const min = Math.min(...points);
      const max = Math.max(...points, min + 0.001);
      const range = max - min;

      const coords = points.map((val, idx) => {
        const x = (idx / (points.length - 1)) * width;
        const y = height - 4 - ((val - min) / range) * (height - 8);
        return [x, y];
      });

      let pathD = `M ${coords[0][0]} ${coords[0][1]}`;
      for (let i = 1; i < coords.length; i++) {
        pathD += ` L ${coords[i][0]} ${coords[i][1]}`;
      }

      // Fill area under sparkline
      const fillD = `${pathD} L ${width} ${height} L 0 ${height} Z`;
      const fillElem = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      fillElem.setAttribute('d', fillD);
      fillElem.setAttribute('fill', fillColor);
      svg.appendChild(fillElem);

      // Stroke line
      const strokeElem = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      strokeElem.setAttribute('d', pathD);
      strokeElem.setAttribute('fill', 'none');
      strokeElem.setAttribute('stroke', strokeColor);
      strokeElem.setAttribute('stroke-width', '2');
      strokeElem.setAttribute('stroke-linecap', 'round');
      svg.appendChild(strokeElem);

      // Dot at latest value
      const last = coords[coords.length - 1];
      const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
      dot.setAttribute('cx', last[0]);
      dot.setAttribute('cy', last[1]);
      dot.setAttribute('r', '3');
      dot.setAttribute('fill', strokeColor);
      svg.appendChild(dot);
    }

    function updateResourcesUI(data) {
      const ramMb = ((data.estimated_memory_bytes || 0) / 1024 / 1024).toFixed(2);
      const ramVal = document.getElementById('res-ram-val');
      if (ramVal) ramVal.textContent = `${ramMb} MB`;
      const ramSub = document.getElementById('res-ram-sub');
      if (ramSub) ramSub.textContent = `CSR: ${data.csr_edges || 0} edges | MemTable: ${data.memtable_edges || 0} edges`;

      const vVal = document.getElementById('res-v-val');
      if (vVal) vVal.textContent = (data.total_vertices || 0).toLocaleString();
      const eVal = document.getElementById('res-e-val');
      if (eVal) eVal.textContent = (data.total_edges || 0).toLocaleString();
      const edgesBreakdown = document.getElementById('res-edges-breakdown');
      if (edgesBreakdown) edgesBreakdown.textContent = `CSR: ${data.csr_edges || 0} | MemTable: ${data.memtable_edges || 0}`;

      const compVal = document.getElementById('res-compaction-val');
      if (compVal) compVal.textContent = (data.compactions_total || 0).toLocaleString();

      const gpuName = document.getElementById('res-gpu-name');
      if (gpuName) gpuName.textContent = data.gpu_backend || 'Apple Metal UMA';
      const gpuDev = document.getElementById('res-gpu-device');
      if (gpuDev) gpuDev.textContent = `Device #${data.gpu_device || 0} | Offload Threshold: ${(data.gpu_threshold || 10000).toLocaleString()} edges`;
      const gpuBadge = document.getElementById('res-gpu-status-badge');
      if (gpuBadge) {
        if (data.gpu_enabled) {
          gpuBadge.className = 'dash-badge green';
          gpuBadge.textContent = 'Active & Ready';
        } else {
          gpuBadge.className = 'dash-badge gray';
          gpuBadge.textContent = 'Disabled (--enable-gpu)';
        }
      }

      const s3Val = document.getElementById('res-s3-val');
      if (s3Val) s3Val.textContent = data.s3_configured ? 'Enabled' : 'Disabled';
      const s3Sub = document.getElementById('res-s3-sub');
      if (s3Sub) s3Sub.textContent = data.s3_configured ? `Bucket: ${data.s3_bucket}` : 'Pass --s3-bucket to activate';

      const qVal = document.getElementById('res-queries-val');
      if (qVal) qVal.textContent = (data.queries_total || 0).toLocaleString();
      const qSub = document.getElementById('res-queries-sub');
      if (qSub) qSub.textContent = `OK: ${data.queries_ok || 0} | Errors: ${data.queries_error || 0}`;

      // Update Sparklines
      const now = Date.now();
      const dt = Math.max((now - lastQueryPollTime) / 1000, 0.5);
      const totalQ = data.queries_total || 0;
      let qps = 0;
      if (lastQueryCount > 0 && totalQ >= lastQueryCount) {
        qps = (totalQ - lastQueryCount) / dt;
      }
      lastQueryCount = totalQ;
      lastQueryPollTime = now;

      telemetryHistory.qps.push(parseFloat(qps.toFixed(1)));
      if (telemetryHistory.qps.length > 20) telemetryHistory.qps.shift();
      const sparkQps = document.getElementById('spark-qps-val');
      if (sparkQps) sparkQps.textContent = `${qps.toFixed(1)} /s`;
      renderSparkline('sparkline-qps', telemetryHistory.qps, '#58a6ff', 'rgba(88, 166, 255, 0.15)');

      const ramFloat = parseFloat(ramMb) || 0;
      telemetryHistory.ram.push(ramFloat);
      if (telemetryHistory.ram.length > 20) telemetryHistory.ram.shift();
      const sparkRam = document.getElementById('spark-ram-val');
      if (sparkRam) sparkRam.textContent = `${ramMb} MB`;
      renderSparkline('sparkline-ram', telemetryHistory.ram, '#bc8cff', 'rgba(188, 140, 255, 0.15)');
      renderSparkline('sparkline-latency', telemetryHistory.latency, '#3fb950', 'rgba(63, 185, 80, 0.15)');
    }

    async function triggerCompaction() {
      const endpoint = document.getElementById('cluster-url').value.trim();
      const btn = document.getElementById('btn-trigger-compact');
      const alertBox = document.getElementById('compaction-alert');
      if (btn) {
        btn.disabled = true;
        btn.innerHTML = '<span class="spinner"></span> Compacting...';
      }
      try {
        const resp = await fetch(`/api/compact?endpoint=${encodeURIComponent(endpoint)}`, { method: 'POST' });
        if (resp.ok) {
          if (alertBox) {
            alertBox.style.display = 'block';
            alertBox.textContent = 'CSR Compaction completed and replicated across cluster!';
            setTimeout(() => { alertBox.style.display = 'none'; }, 4000);
          }
          await fetchResources();
        }
      } catch (err) {
        console.error("Compaction failed:", err);
      } finally {
        if (btn) {
          btn.disabled = false;
          btn.innerHTML = '⚡ Compact CSR Now';
        }
      }
    }

    // Check Cluster Health on Load and periodic background refresh
    async function checkHealth() {
      await fetchClusterStatus();
      await fetchSchema();
      if (currentView === 'resources') {
        await fetchResources();
      }
    }
    checkHealth();
    setInterval(checkHealth, 3000);
  </script>
</body>
</html>
"###;
