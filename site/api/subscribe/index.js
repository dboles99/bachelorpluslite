// Signup capture for bpad.prompt-forge.dev.
// Copyright (C) 2026 Daniel Boles. SPDX-License-Identifier: GPL-3.0-only
//
// Ported from af-site's api/subscribe, deliberately: the two sites share a
// domain and an operator, and two different signup mechanisms would be two
// things to keep working, two places a GDPR request has to be honoured, and
// two sets of addresses to export if the email platform ever changes.
//
// Two jobs, in this order:
//
//   1. Write the address to Table Storage. THIS is the one that matters. The
//      list is in your own storage account, so moving between email platforms
//      later costs an export rather than a migration.
//   2. Forward it to Kit, if a key is configured. Kit does the part that is
//      genuinely hard: deliverability, one-click unsubscribe, bounce and
//      complaint suppression, GDPR requests.
//
// If step 2 fails the request still succeeds, because the address is already
// stored. A signup captured but not yet synced is a five-minute fix later; a
// signup lost because a third-party API was down is gone.
//
// The API key lives in the Static Web App's application settings and is read
// from the environment. It never reaches the browser, which is the whole
// reason this runs server-side rather than the form posting to Kit directly.
//
// **This is the only server-side code in this project, and it is on the
// website rather than in the product.** BachelorPad+ Lite itself makes no
// network connection at all (ADR-0006), and nothing here changes that.

const { TableClient } = require("@azure/data-tables");

const TABLE = "subscribers";

// Two lists, and they must not be confused with each other. The pages say
// that joining one does not join the other, so the tag is what makes that
// true rather than merely stated.
const LISTS = {
  releases: {
    tag: "bpad-lite-releases",
    consent:
      "I want an email when a new version of BachelorPad+ Lite is released. " +
      "My address is stored in Ireland and sent to Kit, who send the mail. " +
      "I can unsubscribe from any message.",
  },
  waitlist: {
    tag: "bpad-plus-waitlist",
    consent:
      "I want to hear about BachelorPad+ early access by email. My address " +
      "is stored in Ireland and sent to Kit, who send the mail. I can " +
      "unsubscribe from any message.",
  },
};

// Bump this whenever any consent wording above changes, and leave old records
// carrying the text they actually agreed to. Rewriting them to the new
// wording would destroy the only evidence of what was consented to.
const CONSENT_VERSION = "2026-09-10";

// Deliberately permissive. The job is to reject obvious rubbish and typos,
// not to adjudicate RFC 5322: an over-strict pattern rejects valid addresses,
// and the confirmation email is the real proof anyway.
const LOOKS_LIKE_EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

module.exports = async function (context, req) {
  const body = req.body || {};
  const email = String(body.email || "").trim().toLowerCase();
  const listName = String(body.list || "").trim();
  const list = LISTS[listName];

  // An unknown list is a bug in the page, not a user error, and guessing one
  // would file somebody under a heading they did not choose.
  if (!list) {
    context.res = {
      status: 400,
      headers: { "Content-Type": "application/json" },
      body: { ok: false, error: "Unknown list." },
    };
    return;
  }

  // GDPR Art 4(11): consent must be a freely given, specific, informed and
  // unambiguous indication by a clear affirmative action. A notice beside the
  // field is none of those; it informs, it does not obtain. So the tick is
  // required here as well as in the browser: a form post can be made without
  // ever loading the page, and the control that matters is the one that
  // cannot be skipped.
  if (body.consent !== true) {
    context.res = {
      status: 400,
      headers: { "Content-Type": "application/json" },
      body: {
        ok: false,
        error: "Please tick the box to confirm you want these emails.",
      },
    };
    return;
  }

  if (!LOOKS_LIKE_EMAIL.test(email) || email.length > 254) {
    context.res = {
      status: 400,
      headers: { "Content-Type": "application/json" },
      body: { ok: false, error: "That does not look like an email address." },
    };
    return;
  }

  let stored = false;
  const conn = process.env.BPAD_STORAGE_CONNECTION;
  if (conn) {
    try {
      const client = TableClient.fromConnectionString(conn, TABLE);
      await client.createTable().catch(() => {});
      // The address is the row key, so a second signup updates rather than
      // duplicating. Partitioned by list, so "who is on the waitlist?" is a
      // partition scan rather than a table scan, and so the two lists cannot
      // be accidentally mailed as one.
      await client.upsertEntity(
        {
          partitionKey: listName,
          rowKey: email,
          email,
          list: listName,
          tag: list.tag,
          confirmed: false,
          createdUtc: new Date().toISOString(),
          // Art 7(1) puts the burden of demonstrating consent on the
          // controller, so what was agreed to is stored rather than just that
          // something was. "They consented" is not a defence if nobody can
          // say to what.
          consent: true,
          consentVersion: CONSENT_VERSION,
          consentText: list.consent,
          consentUtc: new Date().toISOString(),
        },
        "Merge"
      );
      stored = true;
    } catch (err) {
      context.log.error("storage write failed", err.message);
    }
  } else {
    context.log.warn("BPAD_STORAGE_CONNECTION not set - not storing");
  }

  let synced = false;
  const kitKey = process.env.KIT_API_KEY;
  const kitForm = process.env.KIT_FORM_ID;
  if (kitKey) {
    try {
      // v4 upserts on email address, so re-subscribing is not an error.
      const r = await fetch("https://api.kit.com/v4/subscribers", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Kit-Api-Key": kitKey,
        },
        body: JSON.stringify({
          email_address: email,
          fields: { source: "bpad" },
        }),
      });
      synced = r.ok;
      if (!r.ok) {
        context.log.error("kit responded", r.status, await r.text());
      } else {
        if (kitForm) {
          // Adding to a form is what triggers the confirmation email and the
          // double opt-in flow. Without it the subscriber exists but has
          // consented to nothing.
          await fetch(`https://api.kit.com/v4/forms/${kitForm}/subscribers`, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Kit-Api-Key": kitKey,
            },
            body: JSON.stringify({ email_address: email }),
          });
        }

        // Tag by NAME, resolved by CREATING it rather than by listing.
        //
        // af-site learned this the hard way on 2026-09-08: GET /v4/tags is
        // eventually consistent, and six tags created through the API were
        // absent from the listing for over a minute while a PATCH against
        // each id returned them. Resolving a name against that listing finds
        // nothing during the window and drops the tag silently, and the
        // window is exactly when a newly launched form takes its first
        // signups.
        //
        // POST /v4/tags is an upsert: given a name that already exists it
        // returns the existing tag. So creating is both the resolution and
        // the repair. **This matters more here than it did there**, because
        // the tag is what separates the two lists.
        try {
          const tr = await fetch("https://api.kit.com/v4/tags", {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Kit-Api-Key": kitKey,
            },
            body: JSON.stringify({ name: list.tag }),
          });
          if (tr.ok) {
            const id = ((await tr.json()).tag || {}).id;
            if (id) {
              await fetch(`https://api.kit.com/v4/tags/${id}/subscribers`, {
                method: "POST",
                headers: {
                  "Content-Type": "application/json",
                  "X-Kit-Api-Key": kitKey,
                },
                body: JSON.stringify({ email_address: email }),
              });
            } else {
              context.log.warn("tag resolved to no id: " + list.tag);
            }
          } else {
            context.log.warn("could not resolve tag " + list.tag, tr.status);
          }
        } catch (err) {
          // A missing tag is not worth failing a signup over, but it does
          // mean somebody is on a list nothing distinguishes, so it is logged
          // at error rather than warn.
          context.log.error("tagging failed", err.message);
        }
      }
    } catch (err) {
      context.log.error("kit call failed", err.message);
    }
  }

  // Succeed if the address was captured anywhere. Reporting failure to
  // somebody whose address we already hold would invite them to submit again.
  const ok = stored || synced;
  context.res = {
    status: ok ? 200 : 500,
    headers: { "Content-Type": "application/json" },
    body: ok
      ? {
          ok: true,
          message: synced
            ? "Thanks. Check your inbox to confirm."
            : "Thanks. You are on the list.",
        }
      : { ok: false, error: "Could not record that. Please try again." },
  };
};
