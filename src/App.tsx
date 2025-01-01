import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

import "./App.css";

function App() {
  useEffect(() => {
    invoke("init");
  }, []);

  function handleChangeTitle() {
    invoke("change_tray_title", {
      title: "Hello, World!"
    })
  }

  return (
    <div className="container">
      <h1>Menubar App</h1>
      <p>Your content goes here...</p>
      <button type="button" onClick={handleChangeTitle}>Change Tray Title</button>
    </div>
  );
}

export default App;
