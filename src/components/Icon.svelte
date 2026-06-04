<script lang="ts">
  import { ICON_PATHS } from "../lib/icons";

  interface Props {
    name: string;
    size?: number;
    style?: string;
  }
  let { name, size = 18, style = "" }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  {style}
>
  <!-- icon path data is a trusted constant from icons.ts -->
  {@html ICON_PATHS[name] ?? ICON_PATHS.all}
</svg>
