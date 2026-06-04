<script lang="ts">
  import BrandMark from "../components/BrandMark.svelte";
  import Icon from "../components/Icon.svelte";
  import Field from "../components/Field.svelte";
  import Input from "../components/Input.svelte";
  import IconButton from "../components/IconButton.svelte";
  import Button from "../components/Button.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";

  interface Props {
    oncreate: (password: string, hint: string) => Promise<void> | void;
  }
  let { oncreate }: Props = $props();

  let pw = $state("");
  let confirm = $state("");
  let hint = $state("");
  let show = $state(false);
  let err = $state("");
  let busy = $state(false);

  async function submit() {
    if (busy) return;
    if (pw.length < 8) {
      err = "Use at least 8 characters.";
      return;
    }
    if (pw !== confirm) {
      err = "Passwords do not match.";
      return;
    }
    busy = true;
    try {
      await oncreate(pw, hint);
    } catch (e: unknown) {
      console.error("[naravault] create_vault failed:", e);
      const code = (e as { code?: string })?.code;
      const message =
        typeof e === "string"
          ? e
          : (e as { message?: string })?.message ??
            (e != null ? JSON.stringify(e) : null);
      if (code === "ALREADY_EXISTS") {
        err = "A vault already exists. Use the unlock screen or reset the vault from settings.";
      } else if (message) {
        err = `Error: ${message}`;
      } else {
        err = "Could not create the vault. Please try again.";
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="auth-screen">
  <div class="auth-aside">
    <BrandMark size={34} />
    <div class="auth-aside-body">
      <h1>Your secrets,<br />sealed by one key.</h1>
      <p>
        Logins, cards, crypto recovery phrases and private notes — encrypted behind a single
        master password only you know.
      </p>
      <ul class="auth-points">
        <li><Icon name="lock" size={16} /> Zero-knowledge by design</li>
        <li><Icon name="key" size={16} /> One master password unlocks everything</li>
        <li><Icon name="shield" size={16} /> TOTP, seedphrases &amp; cards in one place</li>
      </ul>
    </div>
    <span class="auth-foot mono">NARA://VAULT · v1.0</span>
  </div>

  <div class="auth-main">
    <div class="auth-card">
      <span class="step-tag mono">SET UP · STEP 1 OF 1</span>
      <h2>Create your master password</h2>
      <p class="auth-lede">
        This is the only password you'll need to remember. We can't reset it for you, so make it
        strong and memorable.
      </p>

      <Field label="Master password">
        <Input
          bind:value={pw}
          type={show ? "text" : "password"}
          mono
          autofocus
          oninput={() => (err = "")}
        >
          {#snippet right()}
            <IconButton
              name={show ? "eyeOff" : "eye"}
              size={16}
              onclick={() => (show = !show)}
              title="Show"
            />
          {/snippet}
        </Input>
      </Field>
      <StrengthMeter password={pw} />

      <Field label="Confirm master password">
        <Input
          bind:value={confirm}
          type={show ? "text" : "password"}
          mono
          oninput={() => (err = "")}
          onkeydownEnter={submit}
        />
      </Field>

      <Field label="Hint" hint="Optional — shown on the unlock screen if you forget">
        <Input bind:value={hint} placeholder="Something only you would understand" />
      </Field>

      {#if err}
        <div class="form-err"><Icon name="close" size={14} /> {err}</div>
      {/if}

      <div class="warn-box">
        <Icon name="shield" size={16} />
        <span>
          If you lose this password, your vault cannot be recovered. There is no backdoor.
        </span>
      </div>

      <Button
        variant="primary"
        full
        icon="lock"
        loading={busy}
        onclick={submit}
        style="margin-top:4px">{busy ? "Creating vault…" : "Create vault"}</Button
      >
    </div>
  </div>
</div>
