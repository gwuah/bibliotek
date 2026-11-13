import React, { useState, useEffect } from 'react'
import DocumentLink from './components/DocumentLink.jsx'
import { dismissMatch, listMatches, mergeMatch } from './api.js'

const BOTH_HAVE_FILES =
  'Both documents have a PDF, and merging would throw one away, so it was refused. Dismiss the match if they are different works.'

function suggestedSurvivor({ documents }) {
  const weight = (d) => [d.has_file ? 1 : 0, d.highlight_count + d.comment_count + d.note_count]
  const [a, b] = documents
  const [wa, wb] = [weight(a), weight(b)]
  return wb[0] > wa[0] || (wb[0] === wa[0] && wb[1] > wa[1]) ? b.id : a.id
}

const count = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`

function Candidate({ document, chosen, onChoose, name }) {
  return (
    <label className={`duplicate-candidate ${chosen ? 'chosen' : ''}`}>
      <input type="radio" name={name} checked={chosen} onChange={onChoose} />
      <div className="flex flex-col gap-1">
        <DocumentLink id={document.id} className="book-title-link font-medium">{document.title}</DocumentLink>
        <div>
          {document.authors.map(author => (
            <span key={author} className="bg-amber-100 px-2 py-0.5 text-xs rounded mr-1">{author}</span>
          ))}
        </div>
        <div className="research-list-date">
          {document.kind === 'web' ? 'web page' : 'pdf'}
          {' · '}{document.has_file ? (document.original_filename ?? 'file') : 'no file'}
          {' · '}{count(document.highlight_count, 'highlight')}, {count(document.comment_count, 'comment')}, {count(document.note_count, 'note')}
          {' · '}from {document.sources.map(s => s.source).join(', ') || 'nowhere'}
        </div>
      </div>
    </label>
  )
}

function Match({ match, onResolved }) {
  const [survivor, setSurvivor] = useState(() => suggestedSurvivor(match))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(null)
  const { shared_authors: sharedAuthors, same_filename: sameFilename } = match.evidence

  const act = async (request) => {
    setBusy(true)
    setError(null)
    try {
      await request()
      onResolved()
    } catch (err) {
      setError(err.code === 'both_have_files' ? BOTH_HAVE_FILES : err.message)
      setBusy(false)
    }
  }

  return (
    <div className="duplicate-match">
      <p className="research-chapter-item-meta">
        same title “{match.basis}”
        {sharedAuthors.length > 0 && ` · shared authors: ${sharedAuthors.join(', ')}`}
        {sameFilename && ' · same filename'}
      </p>
      {match.documents.map(document => (
        <Candidate
          key={document.id}
          document={document}
          name={`survivor-${match.id}`}
          chosen={survivor === document.id}
          onChoose={() => setSurvivor(document.id)}
        />
      ))}
      {error && <p className="research-error">{error}</p>}
      <div className="research-chapter-actions">
        <button disabled={busy} onClick={() => act(() => dismissMatch(match.id))} className="research-btn research-btn-secondary">
          Not the same
        </button>
        <button disabled={busy} onClick={() => act(() => mergeMatch(match.id, survivor))} className="research-btn research-btn-primary">
          {busy ? 'Working...' : 'Merge into chosen'}
        </button>
      </div>
    </div>
  )
}

export default function DuplicatesPage() {
  const [matches, setMatches] = useState(null)
  const [error, setError] = useState(null)

  const loadMatches = async () => {
    try {
      setError(null)
      setMatches(await listMatches())
    } catch (err) {
      setError(err.message)
    }
  }

  useEffect(() => {
    loadMatches()
  }, [])

  if (error) {
    return <div className="research-container"><p className="research-error">{error}</p></div>
  }

  if (!matches) {
    return <div className="research-container"><div className="research-loading">Loading...</div></div>
  }

  return (
    <div className="research-container">
      <div className="research-main">
        <h2 className="research-heading">Possible duplicates ({matches.length})</h2>
        {matches.length === 0 ? (
          <p className="research-empty">No possible duplicates.</p>
        ) : (
          <div className="research-list">
            {matches.map(match => (
              <Match key={match.id} match={match} onResolved={loadMatches} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
