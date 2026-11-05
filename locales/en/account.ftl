# The optional hosted account: polyph login, logout and cloud.
help-login = Sign in to a Polyph account and link this device to it.
help-logout = Sign out on this device.
help-cloud = Your Polyph account.
help-cloud-status = Show the account, plan and linked devices.
login-email-prompt = Email address:
login-code-prompt = Enter the 8-digit code sent to { $email }:
login-github-open = Open { $url } and enter the code { $code }. Waiting for GitHub…
login-done = Signed in as { $who }. Plan: { $plan }.
login-device-linked = This device is linked: rooms it owns use your plan on the hosted relay.
logout-done = Signed out on this device. It stays linked to the account until you remove it on the account page.
cloud-signed-out = Not signed in to { $server }. Run `polyph login` to sign in; Polyph works without an account.
cloud-account = Signed in as { $who } at { $server }.
cloud-plan = Plan: { $plan }
cloud-limits = Limits: { $members } people per room, rooms kept { $days } days, { $links } spectator links per room.
cloud-unlimited = no limit
cloud-devices = { $count ->
    [one] 1 linked device.
   *[other] { $count } linked devices.
}
cloud-this-linked = This device is linked.
cloud-this-unlinked = This device is not linked; run `polyph login` again to link it.
cloud-team = Team { $name } ({ $role })
cloud-plan-free = Free
cloud-plan-pro = Pro
cloud-plan-team = Team
cloud-error-wrong_code = That code is not right. Check the email and try again.
cloud-error-expired = This sign-in has expired. Start again with `polyph login`.
cloud-error-invalid_email = That is not an email address.
cloud-error-rate_limited = Too many attempts. Wait a while and try again.
cloud-error-mail_failed = The sign-in email could not be sent. Try again later.
cloud-error-denied = The sign-in was declined on GitHub.
cloud-error-github_not_configured = GitHub sign-in is not available on this service; use `polyph login --email`.
cloud-error-unauthorized = Your session has ended. Run `polyph login` again.
