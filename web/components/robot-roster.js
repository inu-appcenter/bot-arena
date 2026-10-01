import { mountShadow } from "../lib/component.js";

class RobotRoster extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/components/robot-roster.css", `
      <section aria-labelledby="roster-title"><div class="heading"><h2 id="roster-title">로봇 현황</h2><span class="eyebrow">6 ROBOTS</span></div><div class="groups"></div><div class="cargo-note"><span class="cargo-dot" aria-hidden="true"></span>칸 하나 = 재활용품 1개 · 최대 <span class="capacity">4</span>개</div></section>`);
    this._rows = new Map();
    const groups = this.root.querySelector(".groups");
    for (const team of ["A", "B"]) {
      const group = document.createElement("div");
      group.className = `group team-${team.toLowerCase()}`;
      const header = document.createElement("div");
      header.className = "team-header";
      header.textContent = `TEAM ${team}`;
      group.append(header);
      const rows = document.createElement("div");
      rows.className = "rows";
      group.append(rows);
      groups.append(group);
      this._rows.set(team, { rows, robots: new Map() });
    }
  }

  set state(state) {
    if (!state) return;
    this.root.querySelector(".capacity").textContent = state.capacity;
    for (const team of ["A", "B"]) {
      const group = this._rows.get(team);
      const robots = state.robots.filter(robot => robot.team === team);
      for (const robot of robots) {
        let row = group.robots.get(robot.id);
        if (!row) {
          row = document.createElement("div");
          row.className = "robot-row";
          const id = document.createElement("span");
          id.className = "robot-id";
          const position = document.createElement("span");
          position.className = "position";
          const cargo = document.createElement("div");
          cargo.className = "cargo";
          const count = document.createElement("span");
          count.className = "cargo-count";
          row.append(id, position, cargo, count);
          group.rows.append(row);
          group.robots.set(robot.id, row);
        }
        row.querySelector(".robot-id").textContent = robot.id;
        row.querySelector(".position").textContent = `(${robot.x}, ${robot.y})`;
        row.querySelector(".cargo-count").textContent = `${robot.cargo}/${state.capacity}`;
        row.setAttribute("aria-label", `${team}팀 ${robot.id}, 위치 ${robot.x}, ${robot.y}, 적재량 ${robot.cargo}개 / ${state.capacity}개`);
        const cargo = row.querySelector(".cargo");
        cargo.replaceChildren(...Array.from({ length: state.capacity }, (_, index) => {
          const dot = document.createElement("span");
          dot.className = index < robot.cargo ? "loaded" : "";
          dot.setAttribute("aria-hidden", "true");
          return dot;
        }));
      }
      for (const [id, row] of group.robots) {
        if (!robots.some(robot => robot.id === id)) { row.remove(); group.robots.delete(id); }
      }
    }
  }
}

customElements.define("robot-roster", RobotRoster);
