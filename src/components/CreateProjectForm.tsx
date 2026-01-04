import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import "./CreateProjectForm.css";

interface CreateProjectFormProps {
  onCreated: () => void;
  onCancel: () => void;
}

export function CreateProjectForm({ onCreated, onCancel }: CreateProjectFormProps) {
  const [name, setName] = useState("");
  const [localPath, setLocalPath] = useState("");
  const [supabaseRef, setSupabaseRef] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Supabase Project Folder",
      });

      if (selected) {
        setLocalPath(selected as string);
        // Auto-fill name from folder name if empty
        if (!name) {
          const folderName = (selected as string).split("/").pop() || "";
          setName(folderName);
        }
      }
    } catch (err) {
      console.error("Failed to select folder:", err);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!name.trim()) {
      setError("Project name is required");
      return;
    }

    if (!localPath.trim()) {
      setError("Local path is required");
      return;
    }

    setIsLoading(true);
    try {
      await api.createProject(
        name.trim(),
        localPath.trim(),
        undefined,
        supabaseRef.trim() || undefined
      );
      onCreated();
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <form className="create-project-form" onSubmit={handleSubmit}>
      <h3>Add Project</h3>

      <div className="form-group">
        <label htmlFor="name">Project Name</label>
        <input
          id="name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="My Supabase Project"
          autoFocus
        />
      </div>

      <div className="form-group">
        <label htmlFor="path">Local Folder</label>
        <div className="path-input">
          <input
            id="path"
            type="text"
            value={localPath}
            onChange={(e) => setLocalPath(e.target.value)}
            placeholder="/path/to/supabase/project"
            readOnly
          />
          <button type="button" onClick={selectFolder} className="browse-btn">
            Browse
          </button>
        </div>
      </div>

      <div className="form-group">
        <label htmlFor="ref">
          Supabase Project Ref <span className="optional">(optional)</span>
        </label>
        <input
          id="ref"
          type="text"
          value={supabaseRef}
          onChange={(e) => setSupabaseRef(e.target.value)}
          placeholder="abcdefghijklmnop"
        />
        <p className="hint">
          Find this in your Supabase project settings
        </p>
      </div>

      {error && <div className="error">{error}</div>}

      <div className="form-actions">
        <button
          type="button"
          onClick={onCancel}
          className="cancel-btn"
          disabled={isLoading}
        >
          Cancel
        </button>
        <button type="submit" className="submit-btn" disabled={isLoading}>
          {isLoading ? "Creating..." : "Create Project"}
        </button>
      </div>
    </form>
  );
}
