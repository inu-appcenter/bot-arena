import { mountShadow } from "../lib/component.js";

class MatchScoreboard extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/components/match-scoreboard.css", `
      <section aria-label="경기 점수와 턴">
        <div class="team team-a"><div class="team-mark" aria-hidden="true">A</div><div class="team-info"><span class="team-title">TEAM A</span><h2 class="name-a">기본 수거 봇</h2><span class="sub">가까운 자원부터 차근차근</span></div><div class="score-wrap"><span class="score score-a">0</span><span class="score-unit">점</span></div></div>
        <div class="turn"><span class="eyebrow">COMPLETED TURN</span><div><span class="turn-value">000</span><span class="turn-limit"> / 200</span></div><span class="turn-note">시작을 기다리는 중</span><progress max="200" value="0" aria-label="완료된 턴"></progress></div>
        <div class="team team-b"><div class="team-mark" aria-hidden="true">B</div><div class="team-info"><span class="team-title">TEAM B</span><h2 class="name-b">분담 전략 봇</h2><span class="sub">목표를 나누고 함께 수거</span></div><div class="score-wrap"><span class="score score-b">0</span><span class="score-unit">점</span></div></div>
      </section>`);
  }

  set snapshot(snapshot) {
    if (!snapshot) return;
    const state = snapshot.state;
    this.root.querySelector(".name-a").textContent = snapshot.bot_names?.A || "기본 수거 봇";
    this.root.querySelector(".name-b").textContent = snapshot.bot_names?.B || "분담 전략 봇";
    this.root.querySelector(".score-a").textContent = state.scores.A;
    this.root.querySelector(".score-b").textContent = state.scores.B;
    this.root.querySelector(".turn-value").textContent = String(state.completed_turn).padStart(3, "0");
    this.root.querySelector(".turn-limit").textContent = ` / ${state.max_turns}`;
    this.root.querySelector(".turn-note").textContent = { idle: "시작을 기다리는 중", running: "두 봇이 전략을 실행하는 중", finished: "경기 종료", failed: "실행 오류로 중단" }[snapshot.status];
    const progress = this.root.querySelector("progress");
    progress.max = state.max_turns;
    progress.value = state.completed_turn;
    progress.setAttribute("aria-valuetext", `${state.max_turns}턴 중 ${state.completed_turn}턴 완료`);
  }
}

customElements.define("match-scoreboard", MatchScoreboard);
