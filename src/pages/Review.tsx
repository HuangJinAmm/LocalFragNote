import { useEffect, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import CardReview from "@/components/Review/CardReview";
import DeckDetail from "@/components/Review/DeckDetail";
import DeckList from "@/components/Review/DeckList";
import { useReviewDecks } from "@/components/Review/hooks";
import type { ReviewDeck } from "@/components/Review/types";
import MobileHeader from "@/components/MobileHeader";

const ReviewPage = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams();
  const { decks } = useReviewDecks();
  const [selectedDeck, setSelectedDeck] = useState<ReviewDeck | null>(null);

  const deckIdParam = params.deckId;
  const isStudy = location.pathname.endsWith("/study");

  useEffect(() => {
    if (deckIdParam) {
      const deck = decks.find((d) => d.id === Number(deckIdParam));
      if (deck) {
        setSelectedDeck(deck);
      }
    } else {
      setSelectedDeck(null);
    }
  }, [deckIdParam, decks]);

  const handleSelectDeck = (deck: ReviewDeck) => {
    navigate(`/review/${deck.id}`);
  };

  const handleBackToList = () => {
    navigate("/review");
  };

  const handleStartReview = () => {
    if (selectedDeck) {
      navigate(`/review/${selectedDeck.id}/study`);
    }
  };

  const showStudy = isStudy && selectedDeck;
  const showDetail = !isStudy && deckIdParam && selectedDeck;

  return (
    <section className="@container w-full min-h-full pb-10 sm:pt-3 md:pt-6">
      <MobileHeader />
      <div className="mx-auto w-full max-w-5xl px-4 sm:px-6">
        {showStudy ? (
          <CardReview deckId={selectedDeck.id} onExit={() => navigate(`/review/${selectedDeck.id}`)} />
        ) : showDetail ? (
          <DeckDetail deck={selectedDeck} onBack={handleBackToList} onStartReview={handleStartReview} />
        ) : (
          <DeckList onSelectDeck={handleSelectDeck} />
        )}
      </div>
    </section>
  );
};

export default ReviewPage;
