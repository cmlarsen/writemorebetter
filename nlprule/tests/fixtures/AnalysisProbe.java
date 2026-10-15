// Independent reference for the selected readings in tests/analysis.rs.
// Requires a JDK and the LanguageTool 6.6 distribution; see docs/ANALYSIS.md.
import org.languagetool.*;
import org.languagetool.language.English;

public class AnalysisProbe {
    public static void main(String[] args) throws Exception {
        JLanguageTool lt = new JLanguageTool(new English());
        String[] probes = {
            "I met O'BRIEN today.",
            "I'm goin' home.",
            "“She’s waiting—again…”\u2028He left.",
            "She walked home.\n\nHe stayed."
        };
        for (String text : probes) {
            System.out.println("INPUT " + text.replace("\n", "\\n"));
            for (AnalyzedSentence sentence : lt.analyzeText(text)) {
                System.out.println("SENTENCE " + sentence.getText().replace("\n", "\\n"));
                for (AnalyzedTokenReadings token : sentence.getTokens()) {
                    if (token.isWhitespace() || token.isSentenceStart()) continue;
                    System.out.print(token.getToken() + " [");
                    for (AnalyzedToken reading : token.getReadings()) {
                        System.out.print(reading.getLemma() + ":" + reading.getPOSTag() + ", ");
                    }
                    System.out.println("] " + token.getChunkTags());
                }
            }
        }
    }
}
