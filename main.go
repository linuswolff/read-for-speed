package main

import (
	"fmt"
	"math"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/mattn/go-runewidth"
)

const (
	defaultWPM    = 300
	minWPM        = 100
	maxWPM        = 1000
	wpmStep       = 20
	pulseInterval = 16 * time.Millisecond

	anchorRatio = 0.43
	guideOffset = 5
	guideInset  = 0
)

type pulseMsg time.Time

type styles struct {
	bg      lipgloss.Style
	word    lipgloss.Style
	focal   lipgloss.Style
	prompt  lipgloss.Style
	guide   lipgloss.Style
	menu    lipgloss.Style
	menuDim lipgloss.Style
}

type model struct {
	width  int
	height int

	input string
	words []string

	index   int
	started bool
	playing bool
	ended   bool

	showMenu bool
	wpm      int

	lastPulse time.Time
	carry     time.Duration

	styles styles
}

func newStyles() styles {
	bg := lipgloss.Color("#090A0D")
	return styles{
		bg:      lipgloss.NewStyle().Background(bg),
		word:    lipgloss.NewStyle().Foreground(lipgloss.Color("#D4D7DE")).Background(bg).Bold(true),
		focal:   lipgloss.NewStyle().Foreground(lipgloss.Color("#FF4D4F")).Background(bg).Bold(true),
		prompt:  lipgloss.NewStyle().Foreground(lipgloss.Color("#C5CAD3")).Background(bg).Bold(true),
		guide:   lipgloss.NewStyle().Foreground(lipgloss.Color("#565B65")).Background(bg),
		menu:    lipgloss.NewStyle().Foreground(lipgloss.Color("#7E8591")).Background(bg),
		menuDim: lipgloss.NewStyle().Foreground(lipgloss.Color("#5F6672")).Background(bg),
	}
}

func initialModel() model {
	return model{
		wpm:      defaultWPM,
		showMenu: true,
		styles:   newStyles(),
	}
}

func (m model) Init() tea.Cmd {
	return tea.EnableBracketedPaste
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		return m, nil
	case tea.KeyMsg:
		return m.handleKey(msg)
	case pulseMsg:
		if !m.playing {
			return m, nil
		}
		now := time.Time(msg)
		if m.lastPulse.IsZero() {
			m.lastPulse = now
			return m, pulseCmd()
		}

		m.carry += now.Sub(m.lastPulse)
		m.lastPulse = now
		step := m.wordDuration()

		for m.carry >= step && m.playing {
			m.carry -= step
			if m.index < len(m.words)-1 {
				m.index++
				continue
			}
			m.playing = false
			m.ended = true
			m.carry = 0
		}

		if m.playing {
			return m, pulseCmd()
		}
		return m, nil
	}
	return m, nil
}

func (m model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c":
		return m, tea.Quit
	case "h":
		m.showMenu = !m.showMenu
		return m, nil
	case "up":
		m.wpm = clamp(m.wpm+wpmStep, minWPM, maxWPM)
		return m, nil
	case "down":
		m.wpm = clamp(m.wpm-wpmStep, minWPM, maxWPM)
		return m, nil
	case " ":
		if len(m.words) == 0 || m.ended {
			return m, nil
		}
		if !m.started {
			m.started = true
			m.playing = true
			m.lastPulse = time.Now()
			m.carry = 0
			return m, pulseCmd()
		}
		if m.playing {
			m.playing = false
			m.lastPulse = time.Time{}
			return m, nil
		}
		m.playing = true
		m.lastPulse = time.Now()
		return m, pulseCmd()
	case "enter":
		if !m.playing {
			m.input += " "
			m.resetText(strings.Fields(m.input))
		}
		return m, nil
	case "backspace":
		if !m.playing && len(m.input) > 0 {
			r := []rune(m.input)
			m.input = string(r[:len(r)-1])
			m.resetText(strings.Fields(m.input))
		}
		return m, nil
	}

	if msg.Type == tea.KeyRunes && !m.playing {
		m.input += string(msg.Runes)
		m.resetText(strings.Fields(m.input))
	}

	return m, nil
}

func (m *model) resetText(words []string) {
	m.words = words
	m.index = 0
	m.started = false
	m.playing = false
	m.ended = false
	m.lastPulse = time.Time{}
	m.carry = 0
}

func (m model) View() string {
	if m.width == 0 || m.height == 0 {
		return ""
	}

	rows := m.emptyRows()
	centerY := m.height / 2
	anchorX := m.anchorX()

	m.drawGuides(rows, centerY, anchorX)

	switch {
	case len(m.words) == 0:
		prompt := "Paste some text"
		m.drawAnchoredPrompt(rows, centerY, anchorX, prompt, strings.Index(prompt, "o"))
	case !m.started:
		m.drawAnchoredPrompt(rows, centerY, anchorX, "▶", 0)
	default:
		m.drawWord(rows, centerY, anchorX, m.words[m.index])
	}

	if m.showMenu && !m.playing {
		m.drawMenu(rows)
	}

	canvas := strings.Join(rows, "\n")
	return m.styles.bg.Width(m.width).Height(m.height).Render(canvas)
}

func (m model) emptyRows() []string {
	rows := make([]string, m.height)
	blank := m.styles.bg.Render(strings.Repeat(" ", m.width))
	for i := range rows {
		rows[i] = blank
	}
	return rows
}

func (m model) drawGuides(rows []string, centerY, anchorX int) {
	topY := centerY - guideOffset
	bottomY := centerY + guideOffset

	left := max(0, guideInset)
	right := min(m.width-1, m.width-guideInset-1)
	if right < left {
		return
	}

	anchorX = clamp(anchorX, left, right)
	lineWidth := right - left + 1

	topLine := m.styles.guide.Render(guideLine(lineWidth, anchorX-left, '┬'))
	bottomLine := m.styles.guide.Render(guideLine(lineWidth, anchorX-left, '┴'))

	if topY >= 0 {
		rows[topY] = m.place(left, lineWidth, topLine)
	}
	if bottomY < m.height {
		rows[bottomY] = m.place(left, lineWidth, bottomLine)
	}

	vert := m.styles.guide.Render("│")
	for y := topY + 1; y < centerY-1; y++ {
		if y >= 0 && y < m.height {
			rows[y] = m.place(anchorX, 1, vert)
		}
	}
	for y := centerY + 2; y < bottomY; y++ {
		if y >= 0 && y < m.height {
			rows[y] = m.place(anchorX, 1, vert)
		}
	}
}

func (m model) drawAnchoredPrompt(rows []string, y, anchorX int, text string, focalIdx int) {
	rendered, focalOffset, width := m.renderTextWithFocal(text, m.styles.prompt, focalIdx)
	x := anchorX - focalOffset
	if x < 0 {
		x = 0
	}
	if x+width > m.width {
		x = m.width - width
		if x < 0 {
			x = 0
		}
	}
	rows[y] = m.place(x, width, rendered)
}

func (m model) drawWord(rows []string, y, anchorX int, word string) {
	styledWord, focalOffset, wordWidth := m.renderWord(word)
	x := anchorX - focalOffset
	if x < 0 {
		x = 0
	}
	if x+wordWidth > m.width {
		x = m.width - wordWidth
		if x < 0 {
			x = 0
		}
	}
	rows[y] = m.place(x, wordWidth, styledWord)
}

func (m model) drawMenu(rows []string) {
	if m.height < 1 {
		return
	}

	left := m.styles.menu.Render("Space play/pause") + m.styles.menuDim.Render("  ·  h hide")
	right := m.styles.menu.Render(fmt.Sprintf("↑↓ %d wpm", m.wpm))

	leftW := lipgloss.Width(left)
	rightW := lipgloss.Width(right)
	gap := m.width - leftW - rightW
	if gap < 1 {
		gap = 1
	}

	row := left + m.styles.bg.Render(strings.Repeat(" ", gap)) + right
	rows[m.height-1] = m.place(0, m.width, row)
}

func (m model) renderWord(word string) (rendered string, focalOffset int, wordWidth int) {
	runes := []rune(word)
	if len(runes) == 0 {
		return "", 0, 0
	}
	return m.renderTextWithFocal(word, m.styles.word, focalIndex(len(runes)))
}

func (m model) renderTextWithFocal(text string, normal lipgloss.Style, focalIdx int) (rendered string, focalOffset int, width int) {
	runes := []rune(text)
	if len(runes) == 0 {
		return "", 0, 0
	}
	focalIdx = clamp(focalIdx, 0, len(runes)-1)

	var b strings.Builder
	for i, r := range runes {
		part := string(r)
		if i < focalIdx {
			focalOffset += runewidth.StringWidth(part)
		}
		if i == focalIdx {
			b.WriteString(m.styles.focal.Render(part))
		} else {
			b.WriteString(normal.Render(part))
		}
		width += runewidth.StringWidth(part)
	}
	return b.String(), focalOffset, width
}

func (m model) wordDuration() time.Duration {
	return time.Minute / time.Duration(m.wpm)
}

func (m model) anchorX() int {
	if m.width <= 0 {
		return 0
	}
	return clamp(int(float64(m.width-1)*anchorRatio), 0, m.width-1)
}

func focalIndex(length int) int {
	if length <= 1 {
		return 0
	}
	idx := int(math.Round(float64(length-1) * 0.35))
	return clamp(idx, 0, length-1)
}

func guideLine(width, jointX int, joint rune) string {
	if width <= 0 {
		return ""
	}
	line := []rune(strings.Repeat("─", width))
	if jointX >= 0 && jointX < width {
		line[jointX] = joint
	}
	return string(line)
}

func (m model) place(x, contentWidth int, content string) string {
	if m.width <= 0 {
		return ""
	}
	x = clamp(x, 0, m.width)
	right := m.width - x - contentWidth
	if right < 0 {
		right = 0
	}
	leftPad := m.styles.bg.Render(strings.Repeat(" ", x))
	rightPad := m.styles.bg.Render(strings.Repeat(" ", right))
	return leftPad + content + rightPad
}

func pulseCmd() tea.Cmd {
	return tea.Tick(pulseInterval, func(t time.Time) tea.Msg { return pulseMsg(t) })
}

func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func main() {
	p := tea.NewProgram(initialModel(), tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		fmt.Println("error:", err)
	}
}
