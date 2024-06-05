import { useEffect } from "react";

import "./App.css";
import { invoke } from "@tauri-apps/api/core";

function App() {
  useEffect(() => {
    invoke("init");
  }, []);

  return (
    <div className="container">
      <h1>Menubar App</h1>
      <p>Your content goes here...</p>
    </div>
  );
}

export default App;
