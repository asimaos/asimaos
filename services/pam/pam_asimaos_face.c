/* pam_asimaos_face: sign in by looking at the camera, and approve administrator tasks with a
 * fingerprint or a face chosen in the Asima OS password dialog (asimaos-faced, io.github.asimaos.Face1).
 *
 *   auth  sufficient  pam_asimaos_face.so use=login     login manager (infrared cameras only)
 *   auth  required    pam_asimaos_face.so use=unlock    lock screen: its Face button (kde-face)
 *   auth  sufficient  pam_asimaos_face.so use=admin     polkit-1: the dialog's fingerprint/face token
 *
 * login/unlock: the first sign-in after a start always asks for the password, so the saved
 * passwords (KDE Wallet) can unlock too. A normal webcam never signs in at the login screen; it
 * unlocks the screen and approves admin tasks with two blinks (D47). Without a face set up, the
 * module steps aside and the password is asked as usual.
 *
 * admin (D37): nothing is looked at unless the person tapped the fingerprint or face button of the
 * Asima OS password dialog. The dialog then gets a one-time token from asimaos-faced, which checked
 * the finger or face as root, and sends it in place of the password. This module reads the
 * answer: a token is handed back to asimaos-faced (ConsumeToken); anything else is a password and
 * stays for pam_unix (try_first_pass), so other password dialogs and sudo work as before.
 * A token is only good from the dialog process that asked for it: polkit's helper runs as a
 * socket-activated service, so the process at the other end of its socket (SO_PEERCRED) is the
 * dialog; asimaos-faced compares it with the one the token was given to.
 */
#define _GNU_SOURCE   /* struct ucred */
#define PAM_SM_AUTH
#include <security/pam_ext.h>
#include <security/pam_modules.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>
#include <syslog.h>
#include <systemd/sd-bus.h>

#define TOKEN_PREFIX "asimaos-admin-token:"

/* /etc/asimaos/face.conf: LOGIN=no or UNLOCK=no switches a use off (Asima Manager writes it). */
static int use_enabled(const char *use)
{
    FILE *f = fopen("/etc/asimaos/face.conf", "re");
    char line[128], key[32];
    int enabled = 1;
    snprintf(key, sizeof key, "%s=no", strcmp(use, "unlock") == 0 ? "UNLOCK" : "LOGIN");
    if (!f)
        return 1;
    while (fgets(line, sizeof line, f))
        if (strncmp(line, key, strlen(key)) == 0)
            enabled = 0;
    fclose(f);
    return enabled;
}

/* The password dialog this conversation belongs to: the peer of polkit's helper socket, or the
 * helper's parent when polkit starts it directly (older polkit, setuid helper). */
static unsigned requester(void)
{
    struct ucred cred;
    socklen_t len = sizeof cred;
    if (getsockopt(STDIN_FILENO, SOL_SOCKET, SO_PEERCRED, &cred, &len) == 0 && cred.pid > 0)
        return (unsigned)cred.pid;
    return (unsigned)getppid();
}

static int admin(pam_handle_t *pamh, const char *user)
{
    const char *answer = NULL;
    sd_bus *bus = NULL;
    sd_bus_message *reply = NULL;
    sd_bus_error error = SD_BUS_ERROR_NULL;
    int valid = 0, r;

    /* Asks the usual "Password:" question; the answer stays as PAM_AUTHTOK for pam_unix. */
    if (pam_get_authtok(pamh, PAM_AUTHTOK, &answer, NULL) != PAM_SUCCESS || !answer)
        return PAM_AUTHINFO_UNAVAIL;
    if (strncmp(answer, TOKEN_PREFIX, strlen(TOKEN_PREFIX)) != 0)
        return PAM_IGNORE;   /* a password */

    if (sd_bus_open_system(&bus) < 0)
        return PAM_AUTH_ERR;
    r = sd_bus_call_method(bus, "io.github.asimaos.Face1", "/io/github/asimaos/Face1", "io.github.asimaos.Face1",
                           "ConsumeToken", &error, &reply, "ssu", user, answer, requester());
    if (r >= 0)
        r = sd_bus_message_read(reply, "b", &valid);
    if (r < 0)
        pam_syslog(pamh, LOG_NOTICE, "admin token not checked: %s", error.message ? error.message : strerror(-r));
    sd_bus_error_free(&error);
    sd_bus_message_unref(reply);
    sd_bus_flush_close_unref(bus);
    /* A token is never a password: don't let pam_unix try it. */
    pam_set_item(pamh, PAM_AUTHTOK, NULL);
    if (r >= 0 && valid) {
        pam_syslog(pamh, LOG_INFO, "administrator task approved by fingerprint or face for %s", user);
        return PAM_SUCCESS;
    }
    return PAM_AUTH_ERR;
}

PAM_EXTERN int pam_sm_authenticate(pam_handle_t *pamh, int flags, int argc, const char **argv)
{
    const char *user = NULL, *use = "login";
    sd_bus *bus = NULL;
    sd_bus_message *reply = NULL;
    sd_bus_error error = SD_BUS_ERROR_NULL;
    int match = 0, r;
    (void)flags;

    for (int i = 0; i < argc; i++)
        if (strncmp(argv[i], "use=", 4) == 0)
            use = argv[i] + 4;
    if (pam_get_user(pamh, &user, NULL) != PAM_SUCCESS || !user)
        return PAM_USER_UNKNOWN;
    if (strcmp(use, "admin") == 0)
        return admin(pamh, user);
    if (!use_enabled(use))
        return PAM_IGNORE;

    /* First sign-in since the computer started: password, so KDE Wallet unlocks. */
    if (strcmp(use, "login") == 0) {
        struct passwd *pw = getpwnam(user);
        char run[64];
        struct stat st;
        if (!pw)
            return PAM_USER_UNKNOWN;
        snprintf(run, sizeof run, "/run/user/%u", (unsigned)pw->pw_uid);
        if (stat(run, &st) != 0)
            return PAM_IGNORE;
    }

    if (sd_bus_open_system(&bus) < 0)
        return PAM_AUTHINFO_UNAVAIL;
    {
        char *kind = NULL;
        sd_bus_get_property_string(bus, "io.github.asimaos.Face1", "/io/github/asimaos/Face1", "io.github.asimaos.Face1",
                                   "CameraKind", NULL, &kind);
        pam_info(pamh, kind && strcmp(kind, "webcam") == 0 ? "Look at the camera and blink twice…"
                                                            : "Look at the camera to sign in…");
        free(kind);
    }
    r = sd_bus_call_method(bus, "io.github.asimaos.Face1", "/io/github/asimaos/Face1", "io.github.asimaos.Face1",
                           "VerifyFor", &error, &reply, "ss", user, use);
    if (r >= 0)
        r = sd_bus_message_read(reply, "b", &match);
    if (r < 0 && !sd_bus_error_has_name(&error, "io.github.asimaos.Face1.NoFace"))
        pam_syslog(pamh, LOG_NOTICE, "face sign-in unavailable: %s", error.message ? error.message : strerror(-r));
    sd_bus_error_free(&error);
    sd_bus_message_unref(reply);
    sd_bus_flush_close_unref(bus);
    if (r < 0)
        return PAM_AUTHINFO_UNAVAIL;   /* no face set up, no camera, …: use the password */
    return match ? PAM_SUCCESS : PAM_AUTH_ERR;
}

PAM_EXTERN int pam_sm_setcred(pam_handle_t *pamh, int flags, int argc, const char **argv)
{
    (void)pamh; (void)flags; (void)argc; (void)argv;
    return PAM_SUCCESS;
}
