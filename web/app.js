(function () {
  const app = document.getElementById("app");

  const ICONS = {
    copy: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
    </svg>`,
    pencil: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/>
    </svg>`,
    trash: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <path d="M3 6h18"/>
      <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
      <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
    </svg>`,
    check: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <polyline points="20 6 9 17 4 12"/>
    </svg>`,
    download: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
      <polyline points="7 10 12 15 17 10"/>
      <line x1="12" y1="15" x2="12" y2="3"/>
    </svg>`,
    logout: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
      <polyline points="16 17 21 12 16 7"/>
      <line x1="21" y1="12" x2="9" y2="12"/>
    </svg>`,
    upload: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
      <polyline points="17 8 12 3 7 8"/>
      <line x1="12" y1="3" x2="12" y2="15"/>
    </svg>`,
  };

  function escapeHtml(str) {
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function api(path, options = {}) {
    return fetch(path, {
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      ...options,
    }).then(async (res) => {
      let body = null;
      const text = await res.text();
      try {
        body = text ? JSON.parse(text) : null;
      } catch {
        body = { error: text || "unexpected response" };
      }
      if (!res.ok) {
        const err = new Error((body && body.error) || res.statusText);
        err.status = res.status;
        err.body = body;
        throw err;
      }
      return body;
    });
  }

  function userMessage(err) {
    const status = err.status || 0;
    const body = err.body || {};
    const reason = body.reason || "";
    if (status === 401) return "You are not signed in.";
    if (status === 403) return "You do not own this package.";
    if (status === 409) {
      if (reason.includes("limit") || body.limit) {
        return "Package limit reached (max 5 packages).";
      }
      return "Package name already exists.";
    }
    if (status === 429) return "Daily publish limit reached (one publish or update per day).";
    if (status === 413) return "Package is too large (max 10 MB).";
    if (status === 415) return "Request must be JSON.";
    return err.message || "Something went wrong.";
  }

  function setHtml(element, html) {
    element.innerHTML = html;
  }

  function showError(container, message) {
    setHtml(container, `<div data-testid="error-banner" class="error-banner">${escapeHtml(message)}</div>`);
  }

  function bindCopyButton(buttonId, command, label) {
    const btn = document.getElementById(buttonId);
    if (!btn) return;
    btn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(command);
        btn.innerHTML = ICONS.check;
        btn.setAttribute("aria-label", label + " copied");
        setTimeout(() => {
          btn.innerHTML = ICONS.copy;
          btn.setAttribute("aria-label", "Copy " + label);
        }, 1500);
      } catch {
        alert("Copy failed.");
      }
    });
  }

  // ─── Delete confirmation modal ───
  function showDeleteModal(slug, onConfirm) {
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";
    overlay.innerHTML = `
      <div class="modal-card" role="dialog" aria-modal="true" aria-label="Confirm delete">
        <h2 class="modal-title">Delete package</h2>
        <p class="modal-body">Are you sure you want to delete <strong>${escapeHtml(slug)}</strong>? This action cannot be undone.</p>
        <div class="modal-actions">
          <button id="modalCancel" class="button btn-secondary btn-small">Cancel</button>
          <button id="modalConfirm" class="button btn-danger btn-small" data-testid="package-delete">Delete</button>
        </div>
      </div>`;
    app.appendChild(overlay);

    overlay.querySelector("#modalCancel").addEventListener("click", () => overlay.remove());
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) overlay.remove();
    });
    overlay.querySelector("#modalConfirm").addEventListener("click", () => {
      overlay.remove();
      onConfirm();
    });
  }

  // ─── Router ───
  function route() {
    const path = window.location.pathname;
    if (path === "/" || path === "/index.html") {
      renderLogin();
      return;
    }
    if (path === "/dashboard" || path === "/dashboard.html") {
      renderDashboard();
      return;
    }
    if (path === "/publish" || path === "/publish.html") {
      renderPublish();
      return;
    }
    const detail = /^\/detail\/([^/]+)$/.exec(path);
    if (detail) {
      renderDetail(decodeURIComponent(detail[1]));
      return;
    }
    renderNotFound();
  }

  function renderShell(main) {
    return `
      <div class="page">
        <div class="container container-wide">${main}</div>
      </div>
      <footer>MCM Share</footer>
    `;
  }

  // ─── Login ───
  function renderLogin() {
    setHtml(
      app,
      renderShell(`
        <div class="center login-page">
          <h1 class="hero-title">MCM Share</h1>
          <p class="hero-subtitle">Publish, update, and share Minecraft package files.</p>
          <button data-testid="login-yyid" id="loginBtn" class="btn-primary">Sign in with YY-ID</button>
          <div id="loginError" data-testid="error-banner" class="error-banner login-error hidden"></div>
        </div>
      `)
    );
    const btn = document.getElementById("loginBtn");
    const err = document.getElementById("loginError");
    btn.addEventListener("click", async () => {
      btn.disabled = true;
      err.classList.add("hidden");
      try {
        const data = await api("/api/auth/oidc/start");
        if (data.auth_url && data.auth_url.startsWith("/api/auth/oidc/callback")) {
          await api(data.auth_url);
          window.location.href = "/dashboard";
          return;
        }
        window.location.href = data.auth_url;
      } catch (e) {
        err.textContent = userMessage(e);
        err.classList.remove("hidden");
        btn.disabled = false;
      }
    });
  }

  // ─── Dashboard ───
  async function renderDashboard() {
    setHtml(
      app,
      renderShell(`
        <div class="header">
          <h1>Packages</h1>
          <div class="header-actions">
            <span id="userPill" data-testid="session-owner" class="user-pill"></span>
            <button id="logoutBtn" data-testid="logout" class="button btn-secondary btn-small">${ICONS.logout} Sign out</button>
            <a href="/publish" class="button btn-primary">Publish</a>
          </div>
        </div>
        <div id="errorArea"></div>
        <div id="myPackagesSection"></div>
        <div id="publicPackagesSection"></div>
      `)
    );

    const errorArea = document.getElementById("errorArea");
    const mySection = document.getElementById("myPackagesSection");
    const publicSection = document.getElementById("publicPackagesSection");
    const userPill = document.getElementById("userPill");
    const logoutBtn = document.getElementById("logoutBtn");

    // Logout handler
    logoutBtn.addEventListener("click", async () => {
      try {
        await api("/api/auth/oidc/logout");
      } catch {
        // Best effort — clear session cookie client-side regardless
      }
      window.location.href = "/";
    });

    // Check session
    let currentOwner = null;
    try {
      const session = await api("/api/auth/oidc/session");
      currentOwner = session.owner;
      userPill.textContent = session.owner;
    } catch (e) {
      if (e.status === 401) {
        // Not logged in — hide my-packages, hide logout, show public only
        userPill.textContent = "Guest";
        logoutBtn.classList.add("hidden");
        mySection.classList.add("hidden");
      } else {
        userPill.textContent = "?";
      }
    }

    // Load public packages
    setHtml(publicSection, `<div class="loading"><div class="spinner"></div> Loading packages...</div>`);

    try {
      const data = await api("/api/share/list");
      const pkgs = data.packages || [];

      if (pkgs.length === 0) {
        if (currentOwner) {
          setHtml(
            mySection,
            `<div class="section-block">
              <h2 class="section-title">My Packages</h2>
              <div data-testid="my-packages" class="empty-state">
                <p>You have not published any packages yet.</p>
                <a href="/publish" class="button btn-primary">Publish your first package</a>
              </div>
            </div>`
          );
        }
        setHtml(
          publicSection,
          `<div class="section-block">
            <h2 class="section-title">Public Packages</h2>
            <div data-testid="public-packages" class="empty-state">
              <p>No packages published yet.</p>
            </div>
          </div>`
        );
      } else {
        // Separate into my packages and public packages
        const myPkgs = currentOwner ? pkgs.filter((p) => p.owner === currentOwner) : [];
        const otherPkgs = currentOwner ? pkgs.filter((p) => p.owner !== currentOwner) : pkgs;

        // Render my packages section (only if logged in)
        if (currentOwner && myPkgs.length > 0) {
          setHtml(
            mySection,
            `<div class="section-block">
              <h2 class="section-title">My Packages</h2>
              <div data-testid="my-packages" class="package-list">
                ${myPkgs.map((p) => packageRow(p, true)).join("")}
              </div>
            </div>`
          );
          bindDeleteButtons(mySection);
        } else if (currentOwner) {
          setHtml(
            mySection,
            `<div class="section-block">
              <h2 class="section-title">My Packages</h2>
              <div data-testid="my-packages" class="empty-state">
                <p>You have not published any packages yet.</p>
                <a href="/publish" class="button btn-primary">Publish your first package</a>
              </div>
            </div>`
          );
        }

        // Render public packages section
        const publicPkgs = currentOwner ? otherPkgs : pkgs;
        if (publicPkgs.length === 0 && currentOwner) {
          setHtml(
            publicSection,
            `<div class="section-block">
              <h2 class="section-title">Public Packages</h2>
              <div data-testid="public-packages" class="empty-state">
                <p>No other packages published yet.</p>
              </div>
            </div>`
          );
        } else {
          setHtml(
            publicSection,
            `<div class="section-block">
              <h2 class="section-title">Public Packages</h2>
              <div data-testid="public-packages" class="package-list">
                ${publicPkgs.map((p) => packageRow(p, false)).join("")}
              </div>
            </div>`
          );
          bindDeleteButtons(publicSection);
        }
      }
    } catch (e) {
      showError(errorArea, userMessage(e));
      setHtml(publicSection, "");
    }
  }

  function packageRow(p, isOwner) {
    const installCmd = `curl -fsSL https://mc.dyyapp.com/install/pkg/${encodeURIComponent(p.slug)} | bash`;
    return `
      <div class="package-item">
        <div class="package-meta">
          <p class="package-name">${escapeHtml(p.slug)}</p>
          <p class="package-version">${escapeHtml(p.version)} · ${escapeHtml(p.owner)} · ${escapeHtml(p.updated_at || "")}</p>
        </div>
        <div class="package-actions">
          <div class="install-snippet install-snippet-inline">
            <code>${escapeHtml(installCmd)}</code>
          </div>
          <button class="btn-icon copy-install-btn" data-cmd="${escapeHtml(installCmd)}" data-testid="copy-install-command" aria-label="Copy install command">${ICONS.copy}</button>
          <a href="/detail/${encodeURIComponent(p.slug)}" class="button btn-secondary btn-small">Detail</a>
          ${isOwner ? `<a href="/publish?slug=${encodeURIComponent(p.slug)}" class="button btn-secondary btn-small" data-testid="package-update">${ICONS.pencil} Update</a>` : ""}
          ${isOwner ? `<button data-slug="${escapeHtml(p.slug)}" class="btn-danger btn-small delete-btn" data-testid="package-delete" aria-label="Delete ${escapeHtml(p.slug)}">${ICONS.trash}</button>` : ""}
        </div>
      </div>`;
  }

  function bindDeleteButtons(container) {
    container.querySelectorAll(".delete-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        showDeleteModal(btn.dataset.slug, async () => {
          try {
            await api(`/api/share/pkg/${encodeURIComponent(btn.dataset.slug)}`, { method: "DELETE" });
            renderDashboard();
          } catch (e) {
            const errorArea = document.getElementById("errorArea");
            if (errorArea) showError(errorArea, userMessage(e));
          }
        });
      });
    });
    container.querySelectorAll(".copy-install-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(btn.dataset.cmd);
          btn.innerHTML = ICONS.check;
          setTimeout(() => { btn.innerHTML = ICONS.copy; }, 1500);
        } catch {
          alert("Copy failed.");
        }
      });
    });
  }

  // ─── Publish / Update ───
  function renderPublish() {
    const params = new URLSearchParams(window.location.search);
    const editingSlug = params.get("slug") || "";
    const isEdit = Boolean(editingSlug);

    setHtml(
      app,
      renderShell(`
        <div class="header">
          <h1>${isEdit ? "Update package" : "Publish package"}</h1>
          <div class="header-actions">
            <a href="/dashboard" class="button btn-secondary">Dashboard</a>
          </div>
        </div>
        <div id="status"></div>
        <div id="errorArea"></div>
        <form id="publishForm" class="card">
          <div data-testid="package-upload" class="upload-zone" id="uploadZone">
            <div class="upload-zone-content">
              <span class="upload-icon">${ICONS.upload}</span>
              <p>Drag a .mcm file here, or click to select</p>
              <input type="file" id="fileInput" accept=".mcm,application/json" class="hidden-file-input" />
            </div>
            <p id="fileName" class="upload-filename hidden"></p>
          </div>
          <div class="form-group">
            <label for="slug">Package slug</label>
            <input id="slug" name="slug" type="text" placeholder="my-modpack" ${isEdit ? "readonly" : ""} required value="${escapeHtml(editingSlug)}" />
          </div>
          <div class="form-group">
            <label for="version">Version</label>
            <input id="version" name="version" type="text" placeholder="1.0.0" required />
          </div>
          <div class="form-group">
            <label for="content">Content (JSON)</label>
            <textarea id="content" name="content" placeholder='{"schema_version": 1, ...}' required></textarea>
          </div>
          <div class="actions actions-right">
            <button type="submit" id="submitBtn" class="btn-primary">${isEdit ? "Update" : "Publish"}</button>
          </div>
        </form>
      `)
    );

    const form = document.getElementById("publishForm");
    const status = document.getElementById("status");
    const errorArea = document.getElementById("errorArea");
    const slugInput = document.getElementById("slug");
    const versionInput = document.getElementById("version");
    const contentInput = document.getElementById("content");
    const submitBtn = document.getElementById("submitBtn");
    const uploadZone = document.getElementById("uploadZone");
    const fileInput = document.getElementById("fileInput");
    const fileNameEl = document.getElementById("fileName");

    // File upload zone
    uploadZone.addEventListener("click", () => fileInput.click());
    uploadZone.addEventListener("dragover", (e) => {
      e.preventDefault();
      uploadZone.classList.add("upload-zone-active");
    });
    uploadZone.addEventListener("dragleave", () => uploadZone.classList.remove("upload-zone-active"));
    uploadZone.addEventListener("drop", (e) => {
      e.preventDefault();
      uploadZone.classList.remove("upload-zone-active");
      const file = e.dataTransfer.files[0];
      if (file) loadFile(file);
    });
    fileInput.addEventListener("change", () => {
      if (fileInput.files[0]) loadFile(fileInput.files[0]);
    });

    function loadFile(file) {
      if (!file.name.endsWith(".mcm") && !file.name.endsWith(".json")) {
        showError(errorArea, "Only .mcm or .json files are accepted.");
        return;
      }
      if (file.size > 10 * 1024 * 1024) {
        showError(errorArea, "File is too large (max 10 MB).");
        return;
      }
      fileNameEl.textContent = file.name;
      fileNameEl.classList.remove("hidden");
      const reader = new FileReader();
      reader.onload = () => {
        try {
          const obj = JSON.parse(reader.result);
          contentInput.value = JSON.stringify(obj, null, 2);
          if (obj.name && !slugInput.value) slugInput.value = obj.name;
          if (obj.version && !versionInput.value) versionInput.value = obj.version;
        } catch {
          showError(errorArea, "File does not contain valid JSON.");
        }
      };
      reader.readAsText(file);
    }

    if (isEdit) {
      setHtml(status, `<div class="loading"><div class="spinner"></div> Loading...</div>`);
      api(`/api/share/pkg/${encodeURIComponent(editingSlug)}`)
        .then((data) => {
          versionInput.value = data.version || "";
          contentInput.value = JSON.stringify(data, null, 2);
        })
        .catch((e) => showError(errorArea, userMessage(e)))
        .finally(() => setHtml(status, ""));
    } else {
      contentInput.value = JSON.stringify(
        {
          schema_version: 1,
          name: editingSlug || "sample",
          version: "1.0.0",
          description: "A sample MCM package.",
          game_version: "1.21.1",
          loader: "fabric",
          mods: [],
        },
        null,
        2
      );
    }

    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      setHtml(status, "");
      setHtml(errorArea, "");
      submitBtn.disabled = true;
      submitBtn.textContent = isEdit ? "Updating..." : "Publishing...";

      let contentObj;
      try {
        contentObj = JSON.parse(contentInput.value);
      } catch {
        showError(errorArea, "Content is not valid JSON.");
        submitBtn.disabled = false;
        submitBtn.textContent = isEdit ? "Update" : "Publish";
        return;
      }

      const payload = {
        slug: slugInput.value.trim(),
        version: versionInput.value.trim(),
        content: contentObj,
      };

      try {
        if (isEdit) {
          await api(`/api/share/pkg/${encodeURIComponent(payload.slug)}`, {
            method: "PUT",
            body: JSON.stringify(payload),
          });
        } else {
          await api("/api/share/pkg", {
            method: "POST",
            body: JSON.stringify(payload),
          });
        }
        window.location.href = "/dashboard";
      } catch (e) {
        showError(errorArea, userMessage(e));
        submitBtn.disabled = false;
        submitBtn.textContent = isEdit ? "Update" : "Publish";
      }
    });
  }

  // ─── Detail ───
  async function renderDetail(slug) {
    setHtml(
      app,
      renderShell(`
        <div class="header">
          <h1 id="detailTitle">${escapeHtml(slug)}</h1>
          <div class="header-actions">
            <a href="/dashboard" class="button btn-secondary">Dashboard</a>
          </div>
        </div>
        <div id="errorArea"></div>
        <div id="detailContent"></div>
      `)
    );

    const errorArea = document.getElementById("errorArea");
    const detailContent = document.getElementById("detailContent");
    const installCommand = `curl -fsSL https://mc.dyyapp.com/install/pkg/${encodeURIComponent(slug)} | bash`;

    try {
      const [data, listData] = await Promise.all([
        api(`/api/share/pkg/${encodeURIComponent(slug)}`),
        api("/api/share/list").catch(() => ({ packages: [] })),
      ]);

      const pkgMeta = (listData.packages || []).find((p) => p.slug === slug);
      const packageOwner = pkgMeta ? pkgMeta.owner : (data.owner || null);

      let isOwner = false;
      let currentOwner = null;
      try {
        const session = await api("/api/auth/oidc/session");
        currentOwner = session.owner;
        isOwner = currentOwner && packageOwner && currentOwner === packageOwner;
      } catch {
        // Not logged in
      }

      setHtml(
        detailContent,
        `
        <div class="detail-grid">
          <div class="card detail-section">
            <h2>Package install command</h2>
            <div class="install-snippet">
              <code id="installCommand">${escapeHtml(installCommand)}</code>
              <button id="copyInstall" class="icon-btn" data-testid="copy-install-command" aria-label="Copy install command">${ICONS.copy}</button>
            </div>
          </div>
          ${isOwner ? `
          <div class="card detail-section detail-actions-owner">
            <h2>Manage</h2>
            <div class="actions">
              <a href="/publish?slug=${encodeURIComponent(slug)}" class="button btn-primary btn-small" data-testid="package-update">${ICONS.pencil} Update package</a>
              <button id="detailDeleteBtn" class="button btn-danger btn-small" data-testid="package-delete">${ICONS.trash} Delete package</button>
            </div>
          </div>` : ""}
          <div class="card detail-section">
            <h2>Package metadata</h2>
            <table class="detail-table">
              <tr><td class="detail-label">Slug</td><td>${escapeHtml(data.slug || slug)}</td></tr>
              <tr><td class="detail-label">Version</td><td>${escapeHtml(data.version || "")}</td></tr>
              <tr><td class="detail-label">Owner</td><td>${escapeHtml(data.owner || "")}</td></tr>
              <tr><td class="detail-label">Updated</td><td>${escapeHtml(data.updated_at || "")}</td></tr>
            </table>
          </div>
          <div class="card detail-section">
            <h2>Package JSON</h2>
            <pre id="jsonView">Loading...</pre>
          </div>
        </div>`
      );

      document.getElementById("jsonView").textContent = JSON.stringify(data, null, 2);
      bindCopyButton("copyInstall", installCommand, "install command");

      if (isOwner) {
        document.getElementById("detailDeleteBtn").addEventListener("click", () => {
          showDeleteModal(slug, async () => {
            try {
              await api(`/api/share/pkg/${encodeURIComponent(slug)}`, { method: "DELETE" });
              window.location.href = "/dashboard";
            } catch (e) {
              showError(errorArea, userMessage(e));
            }
          });
        });
      }
    } catch (e) {
      showError(errorArea, userMessage(e));
    }
  }

  // ─── 404 ───
  function renderNotFound() {
    setHtml(
      app,
      renderShell(`
        <div class="center login-page">
          <h1 class="hero-title">Not found</h1>
          <p class="hero-subtitle">This page does not exist.</p>
          <a href="/dashboard" class="button btn-primary">Go to dashboard</a>
        </div>
      `)
    );
  }

  // ─── SPA navigation ───
  window.addEventListener("popstate", route);
  document.addEventListener("click", (e) => {
    const a = e.target.closest("a[href^='/']");
    if (a && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      history.pushState({}, "", a.getAttribute("href"));
      route();
    }
  });

  route();
})();
