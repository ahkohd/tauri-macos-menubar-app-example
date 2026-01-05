import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import * as api from "../api";
import { Organization, RemoteProject } from "../types";
import "./CreateProjectForm.css";

interface CreateProjectFormProps {
  onCreated: () => void;
  onCancel: () => void;
}

type Mode = "create" | "sync";

export function CreateProjectForm({
  onCreated,
  onCancel,
}: CreateProjectFormProps) {
  const [mode, setMode] = useState<Mode>("create");
  const [name, setName] = useState("");
  const [localPath, setLocalPath] = useState("");

  // Create Mode State
  const [orgs, setOrgs] = useState<Organization[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState("");

  // Sync Mode State
  const [remoteProjects, setRemoteProjects] = useState<RemoteProject[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState("");

  const [isLoading, setIsLoading] = useState(false);
  const [isFetchingData, setIsFetchingData] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    setIsFetchingData(true);
    try {
      const hasToken = await api.hasAccessToken();
      if (hasToken) {
        // Fetch orgs and projects in parallel
        const [orgsList, projectsList] = await Promise.all([
          api.listOrganizations().catch(() => []),
          api.listRemoteProjects().catch(() => []),
        ]);

        setOrgs(orgsList);
        if (orgsList.length > 0) {
          setSelectedOrgId(orgsList[0].id);
        }

        setRemoteProjects(projectsList);
        if (projectsList.length > 0) {
          setSelectedProjectId(projectsList[0].id);
        }
      }
    } catch (err) {
      console.error("Failed to load data", err);
    } finally {
      setIsFetchingData(false);
    }
  };

  const selectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Supabase Project Folder",
      });

      if (selected) {
        setLocalPath(selected as string);
        // Auto-fill name from folder name if empty and in Create mode
        if (mode === "create" && !name) {
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

    if (!localPath.trim()) {
      setError("Local path is required");
      return;
    }

    if (mode === "create") {
      if (!name.trim()) {
        setError("Project name is required");
        return;
      }
      if (!selectedOrgId) {
        setError("Organization is required");
        return;
      }
    } else {
      if (!selectedProjectId) {
        setError("Please select a project to sync");
        return;
      }
    }

    setIsLoading(true);
    try {
      if (mode === "create") {
        await api.createProject(
          name.trim(),
          localPath.trim(),
          undefined,
          undefined, // ref not needed for new
          selectedOrgId
        );
      } else {
        // Sync Mode
        const project = remoteProjects.find((p) => p.id === selectedProjectId);
        if (!project) throw new Error("Selected project not found");

        await api.createProject(
          project.name, // Use existing name
          localPath.trim(),
          undefined, // ID not strictly needed if we have ref?
          project.id, // Assuming ID is Ref based on previous analysis
          undefined // Org not needed for link
        );
      }
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

      <div className="mode-toggle">
        <button
          type="button"
          className={mode === "create" ? "active" : ""}
          onClick={() => setMode("create")}
        >
          Create New
        </button>
        <button
          type="button"
          className={mode === "sync" ? "active" : ""}
          onClick={() => setMode("sync")}
        >
          Sync Existing
        </button>
      </div>

      {mode === "create" && (
        <>
          <div className="form-group">
            <label htmlFor="org">Organization</label>
            <select
              id="org"
              value={selectedOrgId}
              onChange={(e) => setSelectedOrgId(e.target.value)}
              disabled={isFetchingData}
            >
              {orgs.map((org) => (
                <option key={org.id} value={org.id}>
                  {org.name}
                </option>
              ))}
              {orgs.length === 0 && (
                <option disabled>No organizations found</option>
              )}
            </select>
          </div>
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
        </>
      )}

      {mode === "sync" && (
        <div className="form-group">
          <label htmlFor="project">Select Remote Project</label>
          <select
            id="project"
            value={selectedProjectId}
            onChange={(e) => setSelectedProjectId(e.target.value)}
            disabled={isFetchingData}
          >
            {remoteProjects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.id})
              </option>
            ))}
            {remoteProjects.length === 0 && (
              <option disabled>No projects found</option>
            )}
          </select>
        </div>
      )}

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
          {isLoading
            ? mode === "create"
              ? "Creating..."
              : "Syncing..."
            : mode === "create"
            ? "Create Project"
            : "Sync Project"}
        </button>
      </div>
    </form>
  );
}
