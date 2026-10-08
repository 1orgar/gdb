pub const HTML_INDEX: &str = r#"<!DOCTYPE html>
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
  </style>
</head>
<body>

  <!-- Top Navigation -->
  <header>
    <div class="brand">
      <span>⚡ GDB STUDIO</span>
      <span class="brand-badge">v0.3.1</span>
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
          <div class="section-title">Vertex Tags</div>
          <div id="schema-vertices">
            <span class="schema-tag" onclick="setQuery('MATCH (a:User) RETURN a.id, a.name, a.age LIMIT 50;')">User (Tag)</span>
            <span class="schema-tag" onclick="setQuery('MATCH (a:Person) RETURN a LIMIT 50;')">Person</span>
          </div>
          <div class="section-title" style="margin-top: 14px;">Edge Types</div>
          <div id="schema-edges">
            <span class="schema-tag edge" onclick="setQuery('MATCH (a)-[:FOLLOWS]->(b) RETURN a, b LIMIT 50;')">FOLLOWS</span>
            <span class="schema-tag edge" onclick="setQuery('MATCH (a)-[:KNOWS]->(b) RETURN a, b LIMIT 50;')">KNOWS</span>
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
            <button class="btn btn-primary btn-sm" id="run-btn" onclick="executeQuery()">
              <span id="run-spinner" style="display:none;" class="spinner"></span>
              <span>▶ Run (Cmd+↵)</span>
            </button>
          </div>
        </div>
        <div class="editor-container">
          <textarea id="query-input" spellcheck="false" placeholder="Enter openCypher, GQL, or CALL algo query here...">MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.name, b.name LIMIT 50;</textarea>
        </div>
      </div>

      <!-- Results Viewports -->
      <div class="results-pane">
        <div class="results-nav">
          <div class="nav-tabs">
            <div class="nav-tab active" id="tab-btn-graph" onclick="switchView('graph', this)">🕸️ Graph View</div>
            <div class="nav-tab" id="tab-btn-table" onclick="switchView('table', this)">📊 Table View</div>
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
          </div>
        </div>

        <!-- Table Viewport -->
        <div id="table-viewport" class="viewport">
          <table class="data-grid" id="data-table">
            <thead><tr id="table-header"><th>No Data</th></tr></thead>
            <tbody id="table-body"><tr><td>Execute a query to view tabular results</td></tr></tbody>
          </table>
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

            <div id="compaction-alert" style="display:none; padding:10px 14px; border-radius:6px; font-size:12px; background:rgba(63,185,80,0.15); border:1px solid #238636; color:#3fb950;">
              Compaction completed successfully!
            </div>
          </div>
        </div>

        <!-- 3D Unity Viewport -->
        <div id="unity-viewport" class="viewport" style="display:none; width:100%; height:100%; position:relative; background:#080b10;">
          <div id="unity-container" style="width:100%; height:100%; display:flex; flex-direction:column; align-items:center; justify-content:center; position:relative;">
            <canvas id="unity-canvas" style="width:100%; height:100%; display:none;"></canvas>
            <div id="unity-hud" style="width:100%; height:100%; display:flex; flex-direction:column; align-items:center; justify-content:center; padding:30px; text-align:center;">
              <div style="font-size:42px; margin-bottom:14px;">🎮</div>
              <div style="font-weight:700; font-size:16px; color:#58a6ff; margin-bottom:8px;">Unity 3D WebGL Graph Viewport</div>
              <div style="font-size:13px; max-width:540px; line-height:1.6; color:#8b949e;">GPU Instancing 3D topology visualizer for massive graphs (&gt;100k nodes) with dynamic 60 FPS spatial Force-Directed simulation and camera orbit.</div>
              <div id="unity-status-badge" style="margin-top:16px; font-size:12px; background:rgba(88,166,255,0.12); border:1px solid #1f6feb; border-radius:6px; padding:6px 14px; color:#58a6ff;">Ready to render query topology in 3D</div>
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

  <!-- Footer -->
  <footer>
    <div class="footer-left">
      <span id="footer-version">GDB Studio v0.3.1</span>
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
        if (isAlgoQuery || hasAlgoMetrics) {
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
      } else if (viewName === 'cluster') {
        fetchClusterStatus();
      } else if (viewName === 'resources') {
        fetchResources();
      }
    }

    window.onUnityNodeSelected = function(nodeId) {
      console.log("[Unity Bridge] Node selected:", nodeId);
      selectNode(nodeId);
    };

    function renderUnityGraph() {
      const badge = document.getElementById('unity-status-badge');
      const nodes = Array.from(graphNodes.values()).map(n => ({ id: n.id, label: n.label, category: n.category, score: n.score }));
      const edges = graphEdges.map(e => ({ src: e.src, dst: e.dst, type: e.type }));
      const payload = { nodes, edges };

      if (window.unityInstance) {
        window.unityInstance.SendMessage('GraphController', 'ReceiveGraphData', JSON.stringify(payload));
        if (badge) {
          badge.textContent = `Unity WebGL Active: ${nodes.length} nodes, ${edges.length} edges rendered in 3D.`;
          badge.style.color = '#3fb950';
          badge.style.borderColor = '#238636';
        }
        return;
      }

      if (badge) {
        const count = nodes.length;
        if (count > 0) {
          badge.textContent = `Active 3D Viewport: ${count} nodes, ${edges.length} edges loaded. Ready for Unity WebGL / GPU Instancing.`;
          badge.style.color = '#3fb950';
          badge.style.borderColor = '#238636';
        } else {
          badge.textContent = 'Ready to render query topology in 3D';
          badge.style.color = '#58a6ff';
          badge.style.borderColor = '#1f6feb';
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
      if (currentView === 'resources') {
        await fetchResources();
      }
    }
    checkHealth();
    setInterval(checkHealth, 3000);
  </script>
</body>
</html>
"#;
