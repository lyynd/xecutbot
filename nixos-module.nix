{ self, ... }:
{
  config,
  pkgs,
  lib,
  ...
}:
let
  name = "xecut-bot";
  cfg = config.services.${name};
  settingsFormat = pkgs.formats.yaml { };
  configFile = settingsFormat.generate "config.yaml" (
    cfg.settings
    // {
      db = {
        sqlite_path = cfg.dataDir + "/xecut_bot.sqlite?mode=rwc";
      };
    }
  );
in
{
  options.services.xecut-bot = {
    enable = lib.mkEnableOption name;

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/${name}";
      description = "Directory for SQLite files.";
    };

    secretsPath = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Path to the secrets file.";
    };

    settings = lib.mkOption {
      inherit (settingsFormat) type;
      default = { };
      description = "Additional settings for the bot.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.xecut-bot = {
      description = name;
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        ExecStart =
          "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/xecut_bot --config ${configFile}"
          + lib.optionalString (cfg.secretsPath != null) " --config ${cfg.secretsPath}";
        DynamicUser = true;
        Restart = "on-failure";
        Type = "simple";

        ReadWritePaths = [ cfg.dataDir ];

        RuntimeDirectory = name;
        StateDirectory = name;

        PrivateTmp = true;
        PrivateUsers = true;
        PrivateDevices = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        MemoryDenyWriteExecute = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectClock = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectControlGroups = true;
        LockPersonality = true;
        RestrictSUIDSGID = true;
        RemoveIPC = true;
        RestrictRealtime = true;
        ProtectHostname = true;
        CapabilityBoundingSet = "";
        SystemCallFilter = [
          "@system-service"
        ];
        SystemCallArchitectures = "native";
      };
    };
  };
}
