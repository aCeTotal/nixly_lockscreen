{ self }:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.nixly-lockscreen;
  pkg = self.packages.${pkgs.system}.default;

  # True when the system logs in automatically (getty, display manager or
  # greetd autologin). With autologin there is no password boundary anyway,
  # so the locker defaults to screensaver-only mode: matrix rain, any input
  # unlocks straight to the desktop.
  autoLoginActive =
    (config.services.displayManager.autoLogin.enable or false)
    || ((config.services.getty.autologinUser or null) != null)
    || ((config.services.greetd.enable or false)
        && ((config.services.greetd.settings or { }) ? initial_session));
in
{
  options.services.nixly-lockscreen = {
    enable = lib.mkEnableOption "nixly-lockscreen Wayland session locker";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkg;
      description = "nixly-lockscreen package";
    };

    maskCtrlAltDel = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Mask ctrl-alt-del.target so Ctrl+Alt+Del cannot reboot the machine.";
    };

    skipAuth = lib.mkOption {
      type = lib.types.bool;
      default = autoLoginActive;
      defaultText = lib.literalMD "`true` when NixOS autologin is configured";
      description = ''
        Skip the password prompt: show matrix rain only, and unlock straight
        to the desktop on any input. Defaults to true when autologin
        (getty/displayManager/greetd) is active on the system.
      '';
    };

    pamService = lib.mkOption {
      type = lib.types.str;
      default = "nixly-lockscreen";
      description = "PAM service name. Must match NIXLY_LOCKSCREEN_PAM_SERVICE if overridden.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    environment.sessionVariables = lib.mkIf cfg.skipAuth {
      NIXLY_LOCKSCREEN_NO_AUTH = "1";
    };

    security.pam.services.${cfg.pamService} = {
      text = ''
        auth     sufficient ${pkgs.linux-pam}/lib/security/pam_unix.so likeauth try_first_pass
        auth     required   ${pkgs.linux-pam}/lib/security/pam_deny.so
        account  required   ${pkgs.linux-pam}/lib/security/pam_unix.so
        password required   ${pkgs.linux-pam}/lib/security/pam_deny.so
        session  required   ${pkgs.linux-pam}/lib/security/pam_unix.so
      '';
    };

    systemd.services.nixly-lockguard = {
      description = "nixly-lockscreen TTY/sysrq lockdown helper";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-logind.service" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/nixly-lockguard";
        Restart = "always";
        RestartSec = 1;
        User = "root";
        RuntimeDirectory = "nixly-lockguard";
        AmbientCapabilities = [ "CAP_SYS_TTY_CONFIG" "CAP_SYS_ADMIN" ];
        CapabilityBoundingSet = [ "CAP_SYS_TTY_CONFIG" "CAP_SYS_ADMIN" ];
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateNetwork = true;
        ProtectKernelTunables = false;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" "@privileged" ];
        ReadWritePaths = [ "/proc/sys/kernel/sysrq" ];
      };
    };

    systemd.suppressedSystemUnits = lib.mkIf cfg.maskCtrlAltDel [
      "ctrl-alt-del.target"
    ];
  };
}
