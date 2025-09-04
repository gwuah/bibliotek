class Bibliotek {
  constructor() {
    this.metadata = {};
    this.init();
  }

  renderMetadata() {
    const createMetadataItem = (key, value) => {
      const listItem = document.createElement("li");
      listItem.classList.add("expandable");
      listItem.setAttribute("data-expanded", "false");
      listItem.innerHTML = `
<div>
    <span class="expand-icon"><expand-icon></expand-icon></span>
    <span>${key} [${value}]</span>
</div>`;

      let keyToType = {
        authors: "author",
        tags: "tag",
        categories: "category",
        ratings: "rating",
      };
      const subList = createSubList(this.metadata[key], keyToType[key]);
      listItem.appendChild(subList);
      return listItem;
    };

    const createSubList = (value, type) => {
      const subList = document.createElement("ul");
      subList.classList.add("sub-list", "hidden");

      let subListItems = value.map((item) => {
        const listItem = document.createElement("li");
        listItem.innerHTML = `${item[type].name} (${item.count})`;
        return listItem;
      });
      subListItems.forEach((item) => subList.appendChild(item));
      return subList;
    };

    const aggregatesList = document.getElementById("aggregates-list");
    Object.keys(this.metadata).forEach((key) => {
      const listItem = createMetadataItem(key, this.metadata[key].length);
      aggregatesList.appendChild(listItem);
    });
  }

  async initializeEventListeners() {
    const expandableItems = document.querySelectorAll(".expandable");
    expandableItems.forEach((item) => {
      item.addEventListener("click", (e) => {
        e.preventDefault();
        this.toggleExpansion(item);
      });

      const subListItems = item.querySelectorAll(".sub-list li");
      subListItems.forEach((subListItem) => {
        subListItem.addEventListener("click", (e) => {
          e.stopPropagation();
          e.preventDefault();
          console.log("subListItem clicked", subListItem.textContent);
        });
      });
    });
  }

  toggleExpansion(expandableItem) {
    const newState = !(expandableItem.dataset.expanded === "true");

    expandableItem.dataset.expanded = String(newState);

    expandableItem.querySelector(".expand-icon").innerHTML = newState
      ? "<minus-icon></minus-icon>"
      : "<expand-icon></expand-icon>";

    expandableItem.querySelector(".sub-list").style.display = newState
      ? "block"
      : "none";
  }

  async loadMetadata() {
    let response = await fetch("/metadata");
    let data = await response.json();
    this.metadata = data.metadata;
  }

  async init() {
    await this.loadMetadata();
    await this.renderMetadata();
    await this.initializeEventListeners();
  }
}

document.addEventListener("DOMContentLoaded", () => {
  window.app = new Bibliotek();
});
