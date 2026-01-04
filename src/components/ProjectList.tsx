import { useState, useEffect } from "react";
import type { Project } from "../types";
import { ProjectItem } from "./ProjectItem";
import { CreateProjectForm } from "./CreateProjectForm";
import * as api from "../api";
import "./ProjectList.css";

export function ProjectList() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [isLoading, setIsLoading] = useState(true);

  const loadProjects = async () => {
    try {
      const data = await api.getProjects();
      setProjects(data);
    } catch (err) {
      console.error("Failed to load projects:", err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadProjects();
  }, []);

  const handleProjectCreated = () => {
    setShowCreateForm(false);
    loadProjects();
  };

  if (isLoading) {
    return <div className="loading">Loading projects...</div>;
  }

  return (
    <div className="project-list">
      {showCreateForm ? (
        <CreateProjectForm
          onCreated={handleProjectCreated}
          onCancel={() => setShowCreateForm(false)}
        />
      ) : (
        <>
          <button
            className="add-project-btn"
            onClick={() => setShowCreateForm(true)}
          >
            + Add Project
          </button>

          {projects.length === 0 ? (
            <div className="empty-state">
              <p>No projects yet</p>
              <p className="hint">Add a project to start watching for changes</p>
            </div>
          ) : (
            <div className="projects">
              {projects.map((project) => (
                <ProjectItem
                  key={project.id}
                  project={project}
                  onUpdate={loadProjects}
                  onDelete={loadProjects}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
