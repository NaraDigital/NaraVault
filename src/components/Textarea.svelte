<script lang="ts">
  interface Props {
    value: string;
    placeholder?: string;
    rows?: number;
    mono?: boolean;
    oninput?: (v: string) => void;
  }
  let { value = "", placeholder = "", rows = 5, mono = false, oninput }: Props = $props();

  function handle(e: Event) {
    oninput?.((e.target as HTMLTextAreaElement).value);
  }
</script>

<textarea
  class="input textarea{mono ? ' input-mono' : ''}"
  {rows}
  {placeholder}
  {value}
  oninput={handle}
></textarea>
