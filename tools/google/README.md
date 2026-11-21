# fw-google

Typed, offline-testable Google automations: Drive listing and export, Sheets read and write, YouTube channel and video metadata.
Built on `google-api-python-client` and `google-auth`.

```sh
cd tools/google
uv sync
uv run pytest -q            # offline, API clients are mocked
uvx ruff check .            # CI lane (root `uvx ruff check .` covers it too)
uv run fw-google whoami     # credential source, quota project, warnings (never prints a token)
uv run fw-google drive-ls -q "name contains 'report'" -n 10
uv run fw-google drive-export <file-id> out.pdf --format pdf
uv run fw-google sheets-read <spreadsheet-id> "Sheet1!A1:C10"
uv run fw-google sheets-write <spreadsheet-id> "Sheet1!A1:B2" '[[1,2],[3,4]]'
uv run fw-google yt-channel --handle @GoogleDevelopers
uv run fw-google yt-video <video-id> [<video-id> ...]
```

## Credentials

`resolve_credentials` tries Application Default Credentials, then falls back to the active `gcloud` user token.
A `GOOGLE_APPLICATION_CREDENTIALS` value that points at a missing file is ignored and reported in `whoami` warnings.

The `gcloud auth login` token only carries `cloud-platform` and a few more scopes (checked with tokeninfo), so Drive, Sheets and
YouTube calls answer 403 `insufficient authentication scopes` with it. Grant the scopes once, in a browser, with your own OAuth
Desktop client (Cloud Console, APIs and Services, Credentials):

```sh
gcloud auth application-default login --client-id-file=client_secret.json \
  --scopes=openid,https://www.googleapis.com/auth/userinfo.email,https://www.googleapis.com/auth/cloud-platform,https://www.googleapis.com/auth/drive.readonly,https://www.googleapis.com/auth/spreadsheets,https://www.googleapis.com/auth/youtube.readonly
```

Enable only the APIs you use (`drive.googleapis.com`, `sheets.googleapis.com`, `youtube.googleapis.com`); the YouTube Data API
has a free daily quota and does not accept service accounts for channel-owner calls.

## Windows: broken ADC from a stale variable

`GOOGLE_APPLICATION_CREDENTIALS` set at User and Machine level to a file that does not exist makes every ADC lookup fail.
Remove it (the Machine scope needs an elevated PowerShell), then open a new shell:

```powershell
[Environment]::SetEnvironmentVariable('GOOGLE_APPLICATION_CREDENTIALS', $null, 'User')
[Environment]::SetEnvironmentVariable('GOOGLE_APPLICATION_CREDENTIALS', $null, 'Machine')   # elevated
```

Also `GOOGLE_CLOUD_PROJECT` (User) and `CLOUDSDK_CORE_PROJECT` (User) can name different projects; keep them equal so the
quota project is the same everywhere.
