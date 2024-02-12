import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";

import "./App.css";

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
