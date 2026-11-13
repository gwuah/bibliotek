import React from "react";
import { paragraphsOf } from "../../text.js";

export default function Paragraphs({ text }) {
  return (
    <div>
      {paragraphsOf(text).map((paragraph, i) => (
        <p key={i}>{paragraph}</p>
      ))}
    </div>
  );
}
