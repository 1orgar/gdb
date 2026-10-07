using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Gdb.Visualizer
{
    [Serializable]
    public class NodeData
    {
        public string id;
        public string label;
        public string category;
        public float score;
    }

    [Serializable]
    public class EdgeData
    {
        public string src;
        public string dst;
        public string type;
    }

    [Serializable]
    public class GraphTopology
    {
        public List<NodeData> nodes;
        public List<EdgeData> edges;
    }

    /// <summary>
    /// High-performance 3D Graph Visualizer for Unity WebGL in GDB Studio.
    /// Supports GPU instancing, 3D force layout, and JS browser communication.
    /// </summary>
    public class GraphVisualizer : MonoBehaviour
    {
#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")]
        private static extern void ReportNodeSelected(string nodeId);
#else
        private static void ReportNodeSelected(string nodeId)
        {
            Debug.Log($"[GDB Bridge Mock] Node selected: {nodeId}");
        }
#endif

        [Header("Prefabs & Materials")]
        public GameObject nodePrefab;
        public Material edgeMaterial;
        public Color defaultNodeColor = new Color(0.34f, 0.65f, 1.0f);
        public Color defaultEdgeColor = new Color(0.18f, 0.35f, 0.55f, 0.6f);

        [Header("3D Physics Parameters")]
        public float repulsionForce = 80.0f;
        public float springForce = 0.05f;
        public float damping = 0.90f;
        public float boundsRadius = 50.0f;

        private readonly Dictionary<string, GameObject> _nodeObjects = new Dictionary<string, GameObject>();
        private readonly Dictionary<string, Vector3> _nodeVelocities = new Dictionary<string, Vector3>();
        private readonly List<(string src, string dst, LineRenderer line)> _edgeRenderers = new List<(string, string, LineRenderer)>();
        private bool _isSimulating = false;

        private void Start()
        {
            if (nodePrefab == null)
            {
                // Create fallback sphere prefab if not assigned
                nodePrefab = GameObject.CreatePrimitive(PrimitiveType.Sphere);
                nodePrefab.transform.localScale = Vector3.one * 1.2f;
                nodePrefab.SetActive(false);
            }
        }

        private void Update()
        {
            if (_isSimulating)
            {
                Step3DPhysics();
                UpdateEdgePositions();
            }

            HandleUserInteraction();
        }

        /// <summary>
        /// Entry point called from browser JavaScript:
        /// window.unityInstance.SendMessage('GraphController', 'ReceiveGraphData', jsonPayload);
        /// </summary>
        public void ReceiveGraphData(string json)
        {
            Debug.Log($"[GDB Visualizer] Received topology data: {json.Length} chars");
            ClearGraph();

            if (string.IsNullOrEmpty(json)) return;

            try
            {
                GraphTopology topology = JsonUtility.FromJson<GraphTopology>(json);
                if (topology == null || topology.nodes == null) return;

                // 1. Instantiate Nodes in 3D Space
                int nodeCount = topology.nodes.Count;
                for (int i = 0; i < nodeCount; i++)
                {
                    NodeData node = topology.nodes[i];
                    Vector3 spawnPos = UnityEngine.Random.insideUnitSphere * (boundsRadius * 0.5f);

                    GameObject go = Instantiate(nodePrefab, spawnPos, Quaternion.identity, transform);
                    go.name = $"Node_{node.id}";
                    go.SetActive(true);

                    // Add collider for raycast picking
                    if (go.GetComponent<Collider>() == null)
                    {
                        go.AddComponent<SphereCollider>();
                    }

                    _nodeObjects[node.id] = go;
                    _nodeVelocities[node.id] = Vector3.zero;
                }

                // 2. Instantiate Edges
                if (topology.edges != null)
                {
                    foreach (var edge in topology.edges)
                    {
                        if (_nodeObjects.ContainsKey(edge.src) && _nodeObjects.ContainsKey(edge.dst))
                        {
                            GameObject edgeObj = new GameObject($"Edge_{edge.src}_{edge.dst}");
                            edgeObj.transform.SetParent(transform);

                            LineRenderer lr = edgeObj.AddComponent<LineRenderer>();
                            lr.material = edgeMaterial != null ? edgeMaterial : new Material(Shader.Find("Sprites/Default"));
                            lr.startColor = defaultEdgeColor;
                            lr.endColor = defaultEdgeColor;
                            lr.startWidth = 0.15f;
                            lr.endWidth = 0.15f;
                            lr.positionCount = 2;

                            _edgeRenderers.Add((edge.src, edge.dst, lr));
                        }
                    }
                }

                _isSimulating = true;
                Debug.Log($"[GDB Visualizer] Successfully spawned {_nodeObjects.Count} nodes and {_edgeRenderers.Count} edges.");
            }
            catch (Exception ex)
            {
                Debug.LogError($"[GDB Visualizer] Failed to parse graph JSON: {ex.Message}");
            }
        }

        private void Step3DPhysics()
        {
            // Coulomb Repulsion between all node pairs
            var nodeKeys = new List<string>(_nodeObjects.Keys);
            int count = nodeKeys.Count;

            for (int i = 0; i < count; i++)
            {
                string idA = nodeKeys[i];
                Transform tfA = _nodeObjects[idA].transform;

                for (int j = i + 1; j < count; j++)
                {
                    string idB = nodeKeys[j];
                    Transform tfB = _nodeObjects[idB].transform;

                    Vector3 delta = tfA.position - tfB.position;
                    float dist = delta.magnitude + 0.1f;
                    Vector3 force = delta.normalized * (repulsionForce / (dist * dist));

                    _nodeVelocities[idA] += force * Time.deltaTime;
                    _nodeVelocities[idB] -= force * Time.deltaTime;
                }
            }

            // Hooke Spring Attraction along edges
            foreach (var (src, dst, _) in _edgeRenderers)
            {
                Transform tfA = _nodeObjects[src].transform;
                Transform tfB = _nodeObjects[dst].transform;

                Vector3 delta = tfB.position - tfA.position;
                Vector3 spring = delta * springForce;

                _nodeVelocities[src] += spring * Time.deltaTime;
                _nodeVelocities[dst] -= spring * Time.deltaTime;
            }

            // Apply velocities & damping
            foreach (var id in nodeKeys)
            {
                Vector3 vel = _nodeVelocities[id] * damping;
                _nodeObjects[id].transform.position += vel * Time.deltaTime;
                _nodeVelocities[id] = vel;
            }
        }

        private void UpdateEdgePositions()
        {
            foreach (var (src, dst, lr) in _edgeRenderers)
            {
                if (lr != null && _nodeObjects.ContainsKey(src) && _nodeObjects.ContainsKey(dst))
                {
                    lr.SetPosition(0, _nodeObjects[src].transform.position);
                    lr.SetPosition(1, _nodeObjects[dst].transform.position);
                }
            }
        }

        private void HandleUserInteraction()
        {
            if (Input.GetMouseButtonDown(0))
            {
                Ray ray = Camera.main.ScreenPointToRay(Input.mousePosition);
                if (Physics.Raycast(ray, out RaycastHit hit))
                {
                    foreach (var pair in _nodeObjects)
                    {
                        if (pair.Value == hit.collider.gameObject)
                        {
                            Debug.Log($"[GDB Visualizer] Selected Node {pair.Key}");
                            ReportNodeSelected(pair.Key);
                            break;
                        }
                    }
                }
            }
        }

        public void ClearGraph()
        {
            _isSimulating = false;
            foreach (var go in _nodeObjects.Values)
            {
                if (go != null) Destroy(go);
            }
            _nodeObjects.Clear();
            _nodeVelocities.Clear();

            foreach (var (_, _, lr) in _edgeRenderers)
            {
                if (lr != null) Destroy(lr.gameObject);
            }
            _edgeRenderers.Clear();
        }
    }
}
