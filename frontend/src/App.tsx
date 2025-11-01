import { FormEvent, useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import './App.css'

type StitchProgress = {
  stage: 'scanning' | 'normalizing' | 'finalizing' | 'complete' | 'failed'
  current: number
  total: number
  message?: string | null
}

type StitchResult = {
  output: string
  fileCount: number
  durationMs: number
}

type FormState = {
  sourceDir: string
  outputFile: string
  title: string
  author: string
  narrator: string
  album: string
  year: string
  coverImage: string
  bitrate: string
  sampleRate: string
  workers: string
}

const initialForm: FormState = {
  sourceDir: '',
  outputFile: '',
  title: '',
  author: '',
  narrator: '',
  album: '',
  year: '',
  coverImage: '',
  bitrate: '96k',
  sampleRate: '44100',
  workers: '',
}

function App() {
  const [form, setForm] = useState<FormState>(initialForm)
  const [isRunning, setIsRunning] = useState(false)
  const [progress, setProgress] = useState<StitchProgress | null>(null)
  const [logLines, setLogLines] = useState<string[]>([])
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [result, setResult] = useState<StitchResult | null>(null)

  const appendLog = (message: string) => {
    setLogLines((lines) => {
      const stamped = `[${new Date().toLocaleTimeString()}] ${message}`
      const next = [...lines, stamped]
      return next.slice(-250)
    })
  }

  useEffect(() => {
    const unlistenPromise = listen<StitchProgress>(
      'stitch-progress',
      ({ payload }) => {
        setProgress(payload)
        if (payload?.message) {
          appendLog(payload.message)
        }
        if (payload?.stage === 'complete') {
          setIsRunning(false)
        } else if (payload?.stage === 'failed') {
          setIsRunning(false)
        }
      },
    )

    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(console.warn)
    }
  }, [])

  const percentComplete = useMemo(() => {
    if (!progress || progress.total === 0) {
      return 0
    }
    const pct = Math.round((progress.current / progress.total) * 100)
    return Number.isFinite(pct) ? pct : 0
  }, [progress])

  const handleFieldChange = (key: keyof FormState) => (value: string) => {
    setForm((prev) => ({ ...prev, [key]: value }))
  }

  const handlePickSource = async () => {
    const selection = await open({
      directory: true,
      multiple: false,
      title: 'Select chapter directory',
    })
    if (typeof selection === 'string') {
      setForm((prev) => ({ ...prev, sourceDir: selection }))
    }
  }

  const handlePickCover = async () => {
    const selection = await open({
      directory: false,
      multiple: false,
      title: 'Select cover image',
      filters: [
        { name: 'Images', extensions: ['jpg', 'jpeg', 'png'] },
      ],
    })
    if (typeof selection === 'string') {
      setForm((prev) => ({ ...prev, coverImage: selection }))
    }
  }

  const handlePickOutput = async () => {
    const selection = await save({
      title: 'Choose output file',
      defaultPath: form.outputFile || form.sourceDir
        ? `${form.sourceDir.replace(/\/$/, '') || ''}/${inferDefaultName(form.sourceDir)}.m4b`
        : undefined,
      filters: [
        { name: 'Audiobook', extensions: ['m4b'] },
      ],
    })
    if (selection) {
      setForm((prev) => ({ ...prev, outputFile: selection }))
    }
  }

  const inferDefaultName = (sourceDir: string) => {
    if (!sourceDir) return 'audiobook'
    const segments = sourceDir.split(/[\\/]/).filter(Boolean)
    return segments.at(-1) ?? 'audiobook'
  }

  const handleReset = () => {
    setForm(initialForm)
    setProgress(null)
    setLogLines([])
    setErrorMessage(null)
    setResult(null)
  }

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    if (!form.sourceDir.trim()) {
      setErrorMessage('Select a source directory to continue.')
      return
    }

    setIsRunning(true)
    setProgress(null)
    setErrorMessage(null)
    setResult(null)
    appendLog('Starting stitch job...')

    try {
      const payload = buildPayload(form)
      const response = await invoke<StitchResult>('stitch_audiobook_command', {
        options: payload,
      })
      setResult(response)
      appendLog(`Stitching complete: ${response.output}`)
    } catch (err) {
      const message =
        err instanceof Error ? err.message : String(err ?? 'Unknown error')
      setErrorMessage(message)
      appendLog(`Error: ${message}`)
    } finally {
      setIsRunning(false)
    }
  }

  return (
    <div className="app-frame">
      <header className="app-header">
        <h1>Audiobook Stitcher</h1>
        <p>Normalise chapters and produce polished M4B audiobooks.</p>
      </header>

      <main className="layout">
        <form className="form" onSubmit={handleSubmit}>
          <fieldset disabled={isRunning}>
            <legend>Source</legend>
            <div className="field-group">
              <label htmlFor="sourceDir">Chapter directory</label>
              <div className="field-row">
                <input
                  id="sourceDir"
                  value={form.sourceDir}
                  placeholder="Select the folder containing chapter audio files"
                  onChange={(event) =>
                    handleFieldChange('sourceDir')(event.target.value)}
                  required
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={handlePickSource}
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="field-group">
              <label htmlFor="outputFile">Output file</label>
              <div className="field-row">
                <input
                  id="outputFile"
                  value={form.outputFile}
                  placeholder="Defaults to <folder>.m4b inside the source directory"
                  onChange={(event) =>
                    handleFieldChange('outputFile')(event.target.value)}
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={handlePickOutput}
                >
                  Browse
                </button>
              </div>
            </div>
          </fieldset>

          <fieldset disabled={isRunning}>
            <legend>Metadata</legend>
            <div className="grid">
              <label>
                Title
                <input
                  value={form.title}
                  onChange={(event) =>
                    handleFieldChange('title')(event.target.value)}
                />
              </label>
              <label>
                Author
                <input
                  value={form.author}
                  onChange={(event) =>
                    handleFieldChange('author')(event.target.value)}
                />
              </label>
              <label>
                Narrator
                <input
                  value={form.narrator}
                  onChange={(event) =>
                    handleFieldChange('narrator')(event.target.value)}
                />
              </label>
              <label>
                Album
                <input
                  value={form.album}
                  onChange={(event) =>
                    handleFieldChange('album')(event.target.value)}
                />
              </label>
              <label>
                Year
                <input
                  value={form.year}
                  onChange={(event) =>
                    handleFieldChange('year')(event.target.value)}
                />
              </label>
              <label>
                Cover
                <div className="field-row">
                  <input
                    value={form.coverImage}
                    onChange={(event) =>
                      handleFieldChange('coverImage')(event.target.value)}
                  />
                  <button
                    type="button"
                    className="secondary"
                    onClick={handlePickCover}
                  >
                    Browse
                  </button>
                </div>
              </label>
            </div>
          </fieldset>

          <fieldset disabled={isRunning}>
            <legend>Encoding</legend>
            <div className="grid">
              <label>
                Bitrate
                <input
                  value={form.bitrate}
                  onChange={(event) =>
                    handleFieldChange('bitrate')(event.target.value)}
                />
              </label>
              <label>
                Sample rate
                <input
                  value={form.sampleRate}
                  onChange={(event) =>
                    handleFieldChange('sampleRate')(event.target.value)}
                />
              </label>
              <label>
                Workers
                <input
                  value={form.workers}
                  placeholder="Auto"
                  onChange={(event) =>
                    handleFieldChange('workers')(event.target.value)}
                />
              </label>
            </div>
          </fieldset>

          <div className="actions">
            <button type="submit" className="primary" disabled={isRunning}>
              {isRunning ? 'Stitching…' : 'Start stitching'}
            </button>
            <button
              type="button"
              className="secondary"
              disabled={isRunning}
              onClick={handleReset}
            >
              Reset form
            </button>
          </div>
        </form>

        <aside className="status-panel">
          <section>
            <header>
              <h2>Progress</h2>
              <span className={`status-pill status-${progress?.stage ?? 'idle'}`}>
                {progress?.stage ?? 'idle'}
              </span>
            </header>
            <div className="progress-bar">
              <div
                className="progress-bar__fill"
                style={{ width: `${percentComplete}%` }}
              />
            </div>
            <p className="progress-details">
              {progress?.message || 'Waiting to start'}
            </p>
          </section>

          {result && (
            <section className="result">
              <header>
                <h2>Latest result</h2>
              </header>
              <dl>
                <div>
                  <dt>Output</dt>
                  <dd>{result.output}</dd>
                </div>
                <div>
                  <dt>Chapters merged</dt>
                  <dd>{result.fileCount}</dd>
                </div>
                <div>
                  <dt>Elapsed</dt>
                  <dd>{(result.durationMs / 1000).toFixed(1)}s</dd>
                </div>
              </dl>
            </section>
          )}

          {errorMessage && (
            <section className="error">
              <header>
                <h2>Error</h2>
              </header>
              <p>{errorMessage}</p>
            </section>
          )}

          <section className="log">
            <header>
              <h2>Log</h2>
            </header>
            <div className="log-view">
              {logLines.length === 0 ? (
                <p className="log-placeholder">No messages yet.</p>
              ) : (
                <ul>
                  {logLines.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              )}
            </div>
          </section>
        </aside>
      </main>
    </div>
  )
}

function buildPayload(state: FormState) {
  const payload: Record<string, unknown> = {
    sourceDir: state.sourceDir.trim(),
  }
  const optionalFields: Array<[keyof FormState, string]> = [
    ['outputFile', 'outputFile'],
    ['title', 'title'],
    ['author', 'author'],
    ['narrator', 'narrator'],
    ['album', 'album'],
    ['year', 'year'],
    ['coverImage', 'coverImage'],
    ['bitrate', 'bitrate'],
  ]
  optionalFields.forEach(([field, key]) => {
    const value = state[field].trim()
    if (value) payload[key] = value
  })

  const sampleRate = Number.parseInt(state.sampleRate.trim(), 10)
  if (!Number.isNaN(sampleRate)) {
    payload.sampleRate = sampleRate
  }

  const workers = Number.parseInt(state.workers.trim(), 10)
  if (!Number.isNaN(workers)) {
    payload.workers = workers
  }

  return payload
}

export default App
