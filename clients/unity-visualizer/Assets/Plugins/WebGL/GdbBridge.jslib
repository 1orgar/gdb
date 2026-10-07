mergeInto(LibraryManager.library, {
    ReportNodeSelected: function (nodeIdPtr) {
        var nodeId = UTF8ToString(nodeIdPtr);
        console.log("[Unity WebGL Bridge] Node selected: " + nodeId);
        if (window.onUnityNodeSelected) {
            window.onUnityNodeSelected(nodeId);
        }
    }
});
