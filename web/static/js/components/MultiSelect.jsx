import React, { useState, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'

export default function MultiSelect({ options, selected, onChange, onCreate, placeholder, entityType }) {
  const [isOpen, setIsOpen] = useState(false)
  const [search, setSearch] = useState('')
  const [creating, setCreating] = useState(false)
  const [dropdownStyle, setDropdownStyle] = useState(null)
  const containerRef = useRef(null)
  const dropdownRef = useRef(null)

  const updateDropdownPosition = () => {
    if (containerRef.current) {
      const rect = containerRef.current.getBoundingClientRect()
      setDropdownStyle({
        position: 'fixed',
        top: rect.bottom + 4,
        left: rect.left,
        width: rect.width,
        zIndex: 9999
      })
    }
  }

  const openDropdown = () => {
    updateDropdownPosition()
    setIsOpen(true)
  }

  useEffect(() => {
    const handleClickOutside = (e) => {
      if (containerRef.current && !containerRef.current.contains(e.target) &&
        dropdownRef.current && !dropdownRef.current.contains(e.target)) {
        setIsOpen(false)
        setDropdownStyle(null)
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const filtered = options.filter(o =>
    o.name.toLowerCase().includes(search.toLowerCase()) &&
    !selected.some(s => s.id === o.id)
  )

  const handleCreate = async () => {
    if (!search.trim() || creating) return
    setCreating(true)
    const created = await onCreate(search.trim())
    if (created) {
      onChange([...selected, created])
      setSearch('')
    }
    setCreating(false)
  }

  const showCreateOption = search.trim() &&
    !options.some(o => o.name.toLowerCase() === search.toLowerCase()) &&
    !selected.some(s => s.name.toLowerCase() === search.toLowerCase())

  const showDropdown = isOpen && dropdownStyle && (filtered.length > 0 || showCreateOption)

  return (
    <div ref={containerRef} className="relative">
      <div
        className="border border-gray-300 px-1 min-h-[24px] flex flex-wrap gap-1 cursor-text items-center"
        onClick={openDropdown}
      >
        {selected.map(s => (
          <span key={s.id} className="bg-amber-100 px-2 py-0.5 text-xs rounded flex items-center gap-1">
            {s.name}
            <button onClick={(e) => { e.stopPropagation(); onChange(selected.filter(x => x.id !== s.id)) }} className="hover:text-red-500">×</button>
          </span>
        ))}
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onFocus={openDropdown}
          placeholder={selected.length ? '' : placeholder}
          className="flex-1 min-w-[60px] outline-none text-sm px-2 bg-transparent"
        />
      </div>
      {showDropdown && createPortal(
        <div
          ref={dropdownRef}
          style={dropdownStyle}
          className="bg-white border border-gray-300 max-h-40 overflow-auto shadow-lg"
        >
          {filtered.map(o => (
            <div
              key={o.id}
              onClick={() => { onChange([...selected, o]); setSearch('') }}
              className="px-2 py-1 text-sm cursor-pointer hover:bg-gray-100"
            >
              {o.name}
            </div>
          ))}
          {showCreateOption && (
            <div
              onClick={handleCreate}
              className="px-2 py-1 text-sm cursor-pointer hover:bg-green-100 text-green-700 border-t"
            >
              {creating ? 'Creating...' : `Create "${search}"`}
            </div>
          )}
        </div>,
        document.body
      )}
    </div>
  )
}
