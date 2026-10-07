# GDB 3D Unity Visualizer (`clients/unity-visualizer`)

Unity 3D WebGL graph visualizer for **GDB Studio**.

## Architecture & Integration

- **Target**: WebGL 2.0 / WebGPU
- **Container**: Embedded `<canvas id="unity-canvas">` inside GDB Studio
- **Two-way Browser Bridge**:
  - Browser -> Unity: `window.unityInstance.SendMessage('GraphController', 'ReceiveGraphData', jsonString);`
  - Unity -> Browser: `ReportNodeSelected(string nodeId)` via `GdbBridge.jslib` calling `window.onUnityNodeSelected(nodeId)`.

## Building for WebGL

1. Open this directory as a project in **Unity 2022.3+ LTS** or **Unity 6**.
2. Go to **File -> Build Settings...**
3. Select **WebGL** platform and click **Switch Platform**.
4. In **Player Settings -> Resolution and Presentation**:
   - WebGL Template: Minimal
5. Click **Build** and choose output folder `Build/`.
6. Copy the resulting `Build/` folder into `crates/gdb-studio/webgl/` or host on your CDN.
7. In GDB Studio, open the **🎮 3D Unity View** tab to interactively explore graph topology in full 3D space with GPU acceleration.
