# Local AI setup

Keep model weights in each user's OctoSense state directory, outside the Git checkout:

```text
~/.octosense/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf
```

With `OCTOSENSE_HOME` set, use `<OCTOSENSE_HOME>/weights/` instead. Existing `~/.makeos` state and model links are still discovered when `~/.octosense` does not exist; `MAKEOS_HOME` remains a fallback override. To use that directory in the setup commands below, set `OCTOSENSE_HOME="$HOME/.makeos"` first. Commit the setup instructions and app catalog; users install or link their own weights. The repository ignores `*.gguf` and `*.gguf.part` to avoid accidentally checking in large weights or links to personal paths.

## Make the assistant app available

These commands target the project's validated macOS setup and run from the OctoSense repository root. The default catalog launches `makepad-aichat` from `../makepad`, with its model engine enabled by the app's default features. The model file alone does not install that app.

If that sibling checkout does not exist, create a fresh copy and select OctoSense's pinned dependency revision:

```sh
git clone https://github.com/makepad/makepad.git ../makepad
makepad_rev=$(python3 -c 'import json; print(json.load(open("upstream/makepad.json"))["dependency_revision"])')
git -C ../makepad checkout --detach "$makepad_rev"
```

For an existing checkout, keep its revision aligned through the [upstream workflow](upstream.md). A personal app catalog must also contain an `aichat` entry; see [app registration](../README.md#add-an-app). Opening the assistant builds it through Cargo on first launch. It does not require enabling the host's optional `app-aichat` feature.

## Install the tested model

The tested weights are **Qwen3.5 9B, UD-Q4_K_XL GGUF**, about 5.97 GB (5.56 GiB). The download below pins the exact file that matched our local inference check. Its publisher lists Apache-2.0 licensing. See the [Unsloth model card](https://huggingface.co/unsloth/Qwen3.5-9B-GGUF) and [pinned file with checksum](https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/blob/24fadbaba5891f3965d66ea0e2e4aa259cd38c77/Qwen3.5-9B-UD-Q4_K_XL.gguf).

Allow disk space for the weights plus Cargo build artifacts. Inference also needs memory for the model and conversation context; the file size is not a total RAM requirement.

For a new installation, run this block. If the model is already installed or linked at the destination, use the reuse instructions below instead.

```sh
octosense_state_dir="${OCTOSENSE_HOME:-$HOME/.octosense}"
octosense_model="$octosense_state_dir/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf"
mkdir -p "$octosense_state_dir/weights" &&
curl --fail --location --continue-at - \
  --output "$octosense_model.part" \
  "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/24fadbaba5891f3965d66ea0e2e4aa259cd38c77/Qwen3.5-9B-UD-Q4_K_XL.gguf" &&
printf '%s  %s\n' \
  '6f5d30666c2d8ae16a306e616d95341dcf3cc46810df84d7e6f5a7d1e4c1b293' \
  "$octosense_model.part" | shasum -a 256 -c - &&
mv -n "$octosense_model.part" "$octosense_model"
```

Rerun after an interrupted download to resume the `.part` file. The completed file becomes discoverable only after checksum verification succeeds. An existing destination is not replaced. If verification fails, discard only that incomplete `.part` download and retry. The shell never downloads weights automatically at startup.

## Reuse an existing model

To share the fork's existing file, create a link once:

```sh
octosense_state_dir="${OCTOSENSE_HOME:-$HOME/.octosense}"
mkdir -p "$octosense_state_dir/weights" &&
ln -s "$HOME/.makepad/weights/unsloth/Qwen3.5-9B-UD-Q4_K_XL.gguf" \
  "$octosense_state_dir/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf"
```

Keep the source file in place. No symlink is needed if you select its absolute path when launching OctoSense:

```sh
MAKEPAD_AI_CHAT_MODEL="$HOME/.makepad/weights/unsloth/Qwen3.5-9B-UD-Q4_K_XL.gguf" cargo run
```

Replace the path with an existing compatible model on another disk if needed. This variable takes precedence over automatic discovery; an invalid override does not fall back to another file. To make the choice persistent, set it in your local launch script or shell profile. Other GGUF architectures or quantizations are not guaranteed compatible with the pinned runtime.

## Open and verify

Restart OctoSense with `cargo run` and press **F10**. The provider should show **Local · Qwen3.5 9B · local only**. Send a short prompt to check inference; the first answer loads the model. A separate model server or cloud API key is not required for this setup.

The inherited local provider can first look for a resident model service or a Makepad fleet node on the LAN, then load the file on this machine. A brief “listening for the fleet” status is expected. The `local only` setting excludes cloud providers; it does not disable LAN discovery.

If the assistant is unavailable, check the catalog and sibling checkout. If the model is missing, check the effective `OCTOSENSE_HOME`, any `MAKEPAD_AI_CHAT_MODEL` override, and whether the model or symlink target exists. Restart after changing these paths. With multiple discovered GGUFs, the current runtime prefers Qwen files and then the larger file; use the explicit override to select one reliably.

Local inference with the pinned model has been verified. Automated typing through the outer WM pane was not verified; the inference check used the hosted assistant's own controls. See the [validation record](validation.md#shared-local-qwen-model--2026-09-08) for that distinction.
