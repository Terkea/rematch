import { brand } from "../brand";

export default function HomePage() {
  return (
    <main>
      <h1>{brand.wordmark}</h1>
      <p>The web app is running.</p>
    </main>
  );
}
