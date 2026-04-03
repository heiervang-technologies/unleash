const { GoogleGenAI } = require("@google/genai");
const ai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY });
async function run() {
  try {
    const response = await ai.models.generateContent({
      model: "gemini-2.5-flash",
      contents: [
        { role: "user", parts: [{ text: "Hello" }] },
        { role: "user", parts: [{ text: "How are you?" }] }
      ]
    });
    console.log(response.text());
  } catch (e) {
    console.error(e.message);
  }
}
run();
