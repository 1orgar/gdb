    function splitStatements(text) {
      const stmts = [];
      let current = '';
      let inSingleQuote = false;
      let inDoubleQuote = false;
      let inLineComment = false;

      for (let i = 0; i < text.length; i++) {
        const c = text[i];
        const next = i + 1 < text.length ? text[i + 1] : '';

        if (inLineComment) {
          if (c === '\n' || c === '\r') inLineComment = false;
          current += c;
          continue;
        }

        if (c === '/' && next === '/') {
          inLineComment = true;
          current += c;
          continue;
        }

        if (c === '\'' && !inDoubleQuote) {
          inSingleQuote = !inSingleQuote;
          current += c;
          continue;
        }
        if (c === '"' && !inSingleQuote) {
          inDoubleQuote = !inDoubleQuote;
          current += c;
          continue;
        }

        if (c === ';' && !inSingleQuote && !inDoubleQuote) {
          const trimmed = current.trim();
          if (trimmed.length > 0) {
            stmts.push(trimmed);
          }
          current = '';
          continue;
        }
        current += c;
      }
      const trimmed = current.trim();
      if (trimmed.length > 0) {
        stmts.push(trimmed);
      }
      return stmts;
    }

    async function executeBatchScript() {
      const script = document.getElementById('query-input').value.trim();
      const endpoint = document.getElementById('cluster-url').value.trim();
      if (!script) return;

      const runBtn = document.getElementById('run-btn');
      const batchBtn = document.getElementById('batch-btn');
      const spinner = document.getElementById('run-spinner');
      const stepIndicator = document.getElementById('step-indicator');
      const statStep = document.getElementById('stat-step');

      runBtn.disabled = true;
      if (batchBtn) batchBtn.disabled = true;
      spinner.style.display = 'inline-block';
      stepIndicator.style.display = 'inline-block';

      const stmts = splitStatements(script);
      if (stmts.length === 0) {
        runBtn.disabled = false;
        if (batchBtn) batchBtn.disabled = false;
        spinner.style.display = 'none';
        stepIndicator.style.display = 'none';
        return;
      }

      const startTime = performance.now();
      let lastData = null;
      let okCount = 0;

      for (let i = 0; i < stmts.length; i++) {
        statStep.textContent = `${i + 1}/${stmts.length}`;
        document.getElementById('stat-status').textContent = `Running step ${i + 1}/${stmts.length}...`;
        const q = stmts[i];

        try {
          const resp = await fetch('/api/query', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ query: q, endpoint: endpoint })
          });
          const data = await resp.json();
          lastData = data;

          if (data.status !== 'ok') {
            document.getElementById('stat-status').textContent = `Error (Step ${i + 1}/${stmts.length})`;
            document.getElementById('stat-status').style.color = 'var(--danger)';
            document.getElementById('json-output').textContent = JSON.stringify(data, null, 2);
            renderTable(data);
            alert(`Execution failed at statement ${i + 1} of ${stmts.length}:\n\n${q}\n\nError: ${data.error || data.message || 'Unknown error'}`);
            break;
          }
          okCount++;
        } catch (err) {
          document.getElementById('stat-status').textContent = `Network error (Step ${i + 1})`;
          alert(`Network error at statement ${i + 1}: ${err.message}`);
          break;
        }
      }

      const totalDuration = (performance.now() - startTime).toFixed(1);
      document.getElementById('stat-time').textContent = `${totalDuration} ms`;
      if (lastData && okCount === stmts.length) {
        document.getElementById('stat-status').textContent = `OK (${stmts.length} steps)`;
        document.getElementById('stat-status').style.color = 'var(--success)';
        document.getElementById('stat-rows').textContent = lastData.num_rows || (lastData.rows ? lastData.rows.length : 0);
        document.getElementById('json-output').textContent = JSON.stringify(lastData, null, 2);
        renderTable(lastData);
        if (lastData.graph && (lastData.graph.nodes.length > 0 || lastData.graph.edges.length > 0)) {
          buildGraph(lastData.graph.nodes, lastData.graph.edges);
        } else {
          extractGraphFromRows(lastData);
        }
        sync3DGraph();
        fetchSchema();
        addHistory(`[Batch ${stmts.length} stmts] ` + stmts[0], true);
      }

      runBtn.disabled = false;
      if (batchBtn) batchBtn.disabled = false;
      spinner.style.display = 'none';
    }

    // Query Execution
    async function executeQuery() {
      const query = document.getElementById('query-input').value.trim();
      const endpoint = document.getElementById('cluster-url').value.trim();
      if (!query) return;

      const stmts = splitStatements(query);
      if (stmts.length > 1) {
        return executeBatchScript();
      }

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
            if (Array.isArray(cell)) {
              if (cell.length <= 8) {
                td.textContent = `[${cell.map(v => typeof v === 'number' ? Number(v.toFixed(4)) : v).join(', ')}]`;
              } else {
                td.textContent = `[${cell.slice(0, 4).map(v => typeof v === 'number' ? Number(v.toFixed(3)) : v).join(', ')}, ... (${cell.length} dims)]`;
              }
              td.style.fontFamily = 'monospace';
              td.style.color = 'var(--cyan)';
            } else {
              td.textContent = (typeof cell === 'object' && cell !== null) ? JSON.stringify(cell) : cell;
            }
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
      if (!data || !data.columns || !data.rows || data.rows.length === 0) {
        graphNodes = [];
        graphEdges = [];
        selectedEntity = null;
        closeInspector();
        document.getElementById('stat-graph').textContent = '0 nodes, 0 edges';
        if (ctx && canvas) {
          ctx.clearRect(0, 0, canvas.width, canvas.height);
        }
        sync3DGraph();
        if (currentView === 'unity') renderUnityGraph();
        return;
      }

      const MAX_GRAPH_NODES = 250;
      let capped = false;
      const nodeMap = new Map();
      graphNodes = [];
      graphEdges = [];

      // Case 1: Multi-column path traversal (e.g. 1-hop, 2-hop, 3-hop: a -> b -> c -> d)
      if (data.columns.length >= 2 && !data.columns[1].includes('score') && !data.columns[1].includes('distance')) {
        const isRelTypeMiddle = (data.columns.length === 3 &&
          (data.columns[1].toLowerCase().includes('type') ||
           data.columns[1].toLowerCase().includes('rel') ||
           data.columns[1].toLowerCase().includes('edge')));

        for (const row of data.rows) {
          if (graphNodes.length >= MAX_GRAPH_NODES) {
            capped = true;
            break;
          }

          if (isRelTypeMiddle) {
            const uId = String(row[0]);
            const relType = String(row[1]);
            const vId = String(row[2]);

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
              label: relType
            });
          } else {
            // Consecutive hops along path: row[0] -> row[1] -> row[2] -> row[3] ...
            for (let step = 0; step < row.length - 1; step++) {
              const uId = String(row[step]);
              const vId = String(row[step + 1]);
              if (!uId || !vId || uId === 'null' || vId === 'null' || uId === 'undefined' || vId === 'undefined') continue;

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

              const exists = graphEdges.some(e => e.source.id === uId && e.target.id === vId);
              if (!exists) {
                graphEdges.push({
                  source: nodeMap.get(uId),
                  target: nodeMap.get(vId),
                  label: ''
                });
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
        setTimeout(() => {
          resizeUnityCanvas();
          renderUnityGraph();
        }, 50);
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
      const parent = unityCanvas.parentElement;
      const rect = parent ? parent.getBoundingClientRect() : { width: 0, height: 0 };
      const w = rect.width || (parent ? parent.clientWidth : 0) || window.innerWidth;
      const h = rect.height || (parent ? parent.clientHeight : 0) || (window.innerHeight - 80);
      if (w > 0 && h > 0) {
        unityCanvas.width = w * window.devicePixelRatio;
        unityCanvas.height = h * window.devicePixelRatio;
        unityCanvas.style.width = w + 'px';
        unityCanvas.style.height = h + 'px';
        if (unityCtx) {
          unityCtx.setTransform(1, 0, 0, 1, 0, 0);
          unityCtx.scale(window.devicePixelRatio, window.devicePixelRatio);
        }
      }
    }
    window.addEventListener('resize', resizeUnityCanvas);
    setTimeout(resizeUnityCanvas, 60);

    function sync3DGraph() {
      const nodeMap = new Map();
      if (graphNodes.length === 0) {
        const demoNodes = [
          { id: '1', label: 'Cluster-1', properties: { role: 'Coordinator', status: 'Online' } },
          { id: '2', label: 'Cluster-2', properties: { role: 'Peer-Ring', status: 'Active' } },
          { id: '3', label: 'Cluster-3', properties: { role: 'Peer-Ring', status: 'Active' } },
          { id: '4', label: 'Storage', properties: { engine: 'Chunked-CSR', rf: '3' } },
          { id: '5', label: 'Vector-HNSW', properties: { dim: '128', metric: 'Cosine' } },
          { id: '6', label: 'Analytics', properties: { algo: 'PageRank', status: 'Ready' } }
        ];
        unityNodes = demoNodes.map((n, idx) => {
          const angle = (idx / demoNodes.length) * Math.PI * 2;
          const u = {
            id: n.id,
            label: n.label,
            properties: n.properties,
            x: Math.cos(angle) * 120,
            y: (Math.random() - 0.5) * 80,
            z: Math.sin(angle) * 120,
            vx: 0,
            vy: 0,
            vz: 0,
            radius: 14
          };
          nodeMap.set(n.id, u);
          return u;
        });
        unityEdges = [
          { source: nodeMap.get('1'), target: nodeMap.get('2'), label: 'RAFT' },
          { source: nodeMap.get('2'), target: nodeMap.get('3'), label: 'RING' },
          { source: nodeMap.get('3'), target: nodeMap.get('1'), label: 'SYNC' },
          { source: nodeMap.get('1'), target: nodeMap.get('4'), label: 'CSR' },
          { source: nodeMap.get('2'), target: nodeMap.get('5'), label: 'EMB' },
          { source: nodeMap.get('3'), target: nodeMap.get('6'), label: 'MPP' }
        ];
        return;
      }

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

      unityCanvas.addEventListener('touchstart', (e) => {
        if (e.touches.length === 1) {
          unity3DDragging = true;
          unity3DButton = 0;
          unity3DDragStart = { x: e.touches[0].clientX, y: e.touches[0].clientY };
        }
      }, { passive: true });
      window.addEventListener('touchmove', (e) => {
        if (!unity3DDragging || currentView !== 'unity' || e.touches.length !== 1) return;
        const dx = e.touches[0].clientX - unity3DDragStart.x;
        const dy = e.touches[0].clientY - unity3DDragStart.y;
        unity3DDragStart = { x: e.touches[0].clientX, y: e.touches[0].clientY };
        unity3DRotY += dx * 0.008;
        unity3DRotX += dy * 0.008;
        unity3DRotX = Math.max(-Math.PI / 2.1, Math.min(Math.PI / 2.1, unity3DRotX));
      }, { passive: true });
      window.addEventListener('touchend', () => {
        unity3DDragging = false;
      });

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
        if (graphNodes.length > 0) {
          badge.textContent = `${count} Live Nodes in 3D Space | Drag to Orbit | Scroll to Zoom | Click Sphere to Inspect`;
          badge.style.color = '#3fb950';
        } else {
          badge.textContent = `Interactive 3D Spatial Visualizer | Drag to Orbit | Click Sphere | Run query to visualize live data`;
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
      const tabT = document.getElementById('tab-templates');
      const tabH = document.getElementById('tab-history');
      if (tabT) tabT.style.display = tabName === 'templates' ? 'block' : 'none';
      if (tabH) tabH.style.display = tabName === 'history' ? 'block' : 'none';
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

      const clusterEdges = data.cluster_total_edges != null ? data.cluster_total_edges : (data.total_edges || 0);
      const clusterVertices = data.cluster_total_vertices != null ? data.cluster_total_vertices : (data.total_vertices || 0);
      const localEdges = data.local_edges != null ? data.local_edges : (data.node_total_edges != null ? data.node_total_edges : (data.total_edges || 0));

      const vVal = document.getElementById('res-v-val');
      if (vVal) vVal.textContent = clusterVertices.toLocaleString();
      const eVal = document.getElementById('res-e-val');
      if (eVal) eVal.textContent = clusterEdges.toLocaleString();
      const edgesBreakdown = document.getElementById('res-edges-breakdown');
      if (edgesBreakdown) {
        if (data.total_nodes && data.total_nodes > 1) {
          edgesBreakdown.textContent = `Cluster: ${clusterEdges} | Node #${data.node_id || 1}: ${localEdges} (CSR: ${data.csr_edges || 0}, MemTable: ${data.memtable_edges || 0})`;
        } else {
          edgesBreakdown.textContent = `CSR: ${data.csr_edges || 0} | MemTable: ${data.memtable_edges || 0}`;
        }
      }

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
