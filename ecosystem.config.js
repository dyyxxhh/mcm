// ecosystem.config.js — PM2 configuration for mcm serve
//
// Start:  pm2 start ecosystem.config.js
// Stop:   pm2 stop mcm
// Delete: pm2 delete mcm
// Logs:   pm2 logs mcm --lines 100
// Status: pm2 show mcm
//
// Secrets (MCM_OIDC_CLIENT_SECRET) must NOT be stored here.
// Provide them via:
//   1. Shell environment before `pm2 start` (exported in the terminal), or
//   2. A secret file loaded by your deployment tooling (e.g., Docker secrets,
//      systemd EnvironmentFile, or a wrapper script that sources a .env file).

module.exports = {
  apps: [
    {
      name: "mcm",
      script: "mcm",
      args: "serve --mode share --bind 0.0.0.0:8950",
      interpreter: "none",
      cwd: "/mnt/Storage1_xe6x96xb0xe5x8axa0xe5x8dxb7/nas/lucky/dyyl",
      env: {
        MCM_SHARE_DATA_DIR: "/home/usr/.mcm/share",
        // MCM_OIDC_ISSUER: "https://auth.dyyapp.com",
        // MCM_OIDC_CLIENT_ID: "<your-client-id>",
        // MCM_OIDC_CLIENT_SECRET: provide via env or secret file, never commit.
        // MCM_OIDC_REDIRECT_URL: "https://mc.dyyapp.com/api/auth/oidc/callback",
      },
      max_restarts: 10,
      autorestart: true,
      watch: false,
      merge_logs: true,
      log_date_format: "YYYY-MM-DD HH:mm:ss Z",
      error_file: "/home/usr/.pm2/logs/mcm-error.log",
      out_file: "/home/usr/.pm2/logs/mcm-out.log",
    },
  ],
};
