class ExpandFileIcon extends HTMLElement {
  static get observedAttributes() {
    return ["name"];
  }
  connectedCallback() {
    const type = this.getAttribute("name");
    this.innerHTML = `
<svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              fill="currentColor"
              class="bi bi-plus-circle inline"
              viewBox="0 0 16 16"
            >
              <path
                d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14m0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16"
              />
              <path
                d="M8 4a.5.5 0 0 1 .5.5v3h3a.5.5 0 0 1 0 1h-3v3a.5.5 0 0 1-1 0v-3h-3a.5.5 0 0 1 0-1h3v-3A.5.5 0 0 1 8 4"
              />
            </svg>
            ${type}
    `;
  }
}

customElements.define("expand-file-icon", ExpandFileIcon);

class AsciiTreeManager {
  constructor() {
    this.initializeTree();
  }

  initializeTree() {
    // Find all expandable items and add click handlers
    const expandableItems = document.querySelectorAll(
      ".ascii-tree .expandable"
    );

    expandableItems.forEach((item) => {
      // Add class based on number of children for proper line height
      const subList = item.querySelector(".sub-list");
      const childCount = subList.querySelectorAll("li").length;
      item.classList.add(`children-${childCount}`);

      const treeItem = item.querySelector(".tree-item");
      treeItem.addEventListener("click", (e) => {
        e.preventDefault();
        this.toggleExpansion(item);
      });
    });
  }

  toggleExpansion(expandableItem) {
    const isExpanded = expandableItem.dataset.expanded === "true";
    const newState = !isExpanded;

    // Update the data attribute
    expandableItem.dataset.expanded = newState.toString();

    // Update the expand icon
    const expandIcon = expandableItem.querySelector(".expand-icon");
    expandIcon.textContent = newState ? "−" : "+";

    // Get the sub-list
    const subList = expandableItem.querySelector(".sub-list");

    if (newState) {
      // Expanding
      subList.style.display = "block";
      // Trigger reflow to enable transition
      subList.offsetHeight;
      subList.style.opacity = "1";
      subList.style.transform = "translateY(0)";
    } else {
      // Collapsing
      subList.style.opacity = "0";
      subList.style.transform = "translateY(-10px)";
      setTimeout(() => {
        if (expandableItem.dataset.expanded === "false") {
          subList.style.display = "none";
        }
      }, 300);
    }

    // ASCII lines are now handled by CSS automatically
  }

  updateAsciiLines() {
    // ASCII lines are now handled purely by CSS
    // This method can be used for any additional dynamic adjustments if needed
    // Currently no dynamic line creation is necessary
  }

  // Method to dynamically adjust line lengths when content changes
  recalculateLines() {
    // Wait for any ongoing transitions to complete
    setTimeout(() => {
      this.updateAsciiLines();
    }, 350);
  }
}

// Initialize the ASCII tree manager when DOM is loaded
document.addEventListener("DOMContentLoaded", () => {
  window.asciiTreeManager = new AsciiTreeManager();
});
