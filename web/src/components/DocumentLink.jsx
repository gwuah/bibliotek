import React from 'react'
import { documentPath, isPlainClick, navigate } from '../navigation.js'

export default function DocumentLink({ id, children, ...rest }) {
  const handleClick = (e) => {
    if (!isPlainClick(e)) return
    e.preventDefault()
    navigate(documentPath(id))
  }

  return (
    <a href={documentPath(id)} onClick={handleClick} {...rest}>
      {children}
    </a>
  )
}
