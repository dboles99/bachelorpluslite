/* bpad.prompt-forge.dev: the two sign-up forms.
 *
 * There are two lists and they are deliberately separate. The waitlist is for
 * BachelorPad+ early access. The release-notes list is for the free version.
 * Joining one does not join the other, the pages say so, and what makes that
 * true rather than merely stated is the `list` field posted here and the tag
 * the function applies from it.
 *
 * Both post to `/api/subscribe`, which is same-origin: an Azure Function in
 * this Static Web App, matching af-site. Same-origin is not only tidier, it
 * is what lets the Content-Security-Policy stay at `self` with no exception,
 * so this page genuinely makes no third-party request at all (ADR-0076).
 *
 * NOTHING IS STORED IN YOUR BROWSER: no cookie, no storage, no draft of the
 * address. The page names no launch date either, and there is no countdown
 * to one -- a date with no plan behind it is a claim the reader can catch
 * being false (ADR-0089).
 *
 * Every message shown to the reader comes from a data- attribute in the
 * markup rather than from a string in here, so the six locales share one
 * script and a translator never has to open JavaScript.
 */
(function () {
  "use strict";

  var ENDPOINT = "/api/subscribe";

  /* ---- the forms ------------------------------------------------------ */

  function looksLikeEmail(value) {
    var at = value.indexOf("@");
    return at > 0 && value.lastIndexOf(".") > at && value.length > 4;
  }

  function wire(form) {
    var list = form.getAttribute("data-list");
    var button = form.querySelector("button[type=submit]");
    var field = form.querySelector("input[type=email]");
    var consent = form.querySelector("input[type=checkbox][name=consent]");
    var result = form.parentNode.querySelector(".result");
    var busyLabel = form.getAttribute("data-sending") || "Sending";
    var idleLabel = button ? button.textContent : "";

    function say(kind, html) {
      if (!result) { return; }
      result.className = "result " + kind;
      result.innerHTML = html;
      result.hidden = false;
    }

    form.addEventListener("submit", function (event) {
      event.preventDefault();
      if (result) { result.hidden = true; }

      var address = field.value.trim();
      if (!looksLikeEmail(address)) {
        say("bad", form.getAttribute("data-bad-email"));
        field.focus();
        return;
      }

      /* Checked here and again on the server. GDPR Art 4(11) wants a clear
       * affirmative action, and a form post can be made without ever loading
       * this page, so the control that matters is the one in the function.
       * This one exists to say so before a round trip. */
      if (consent && !consent.checked) {
        say("bad", form.getAttribute("data-need-consent"));
        consent.focus();
        return;
      }

      button.disabled = true;
      button.textContent = busyLabel;

      fetch(ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Accept": "application/json" },
        body: JSON.stringify({ email: address, list: list, consent: true })
      }).then(function (response) {
        return response.json().then(function (data) {
          return { ok: response.ok, data: data };
        });
      }).then(function (r) {
        if (!r.ok || !r.data || r.data.ok !== true) {
          throw new Error((r.data && r.data.error) || "failed");
        }
        form.hidden = true;
        say("ok", form.getAttribute("data-ok"));
      }).catch(function (err) {
        button.disabled = false;
        button.textContent = idleLabel;
        /* The server's own wording where there is one, because it is more
         * specific than anything this file could say: a rejected address and
         * an unticked box are different problems with different fixes. */
        var detail = err && err.message && err.message !== "failed"
          ? "<strong>" + err.message + "</strong>"
          : form.getAttribute("data-failed");
        say("bad", detail + form.getAttribute("data-fallback"));
      });
    });
  }

  var forms = document.querySelectorAll("form.js-signup");
  for (var i = 0; i < forms.length; i += 1) { wire(forms[i]); }
})();
