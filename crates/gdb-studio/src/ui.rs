pub const HTML_INDEX: &str = concat!(
    "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"UTF-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n  <title>GDB Studio — Distributed Graph Database Workspace</title>\n  <style>\n",
    include_str!("../web/styles.css"),
    "\n  </style>\n</head>\n<body>\n",
    include_str!("../web/body.html"),
    "\n  <script>\n",
    include_str!("../web/graph.js"),
    "\n",
    include_str!("../web/app.js"),
    "\n  </script>\n</body>\n</html>\n"
);
